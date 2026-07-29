//! mise.toml managed block rendering and sync.

use crate::autogen::replace_named_block_toml;
use crate::managed_mise::BINARY_SPECS;
use crate::managed_mise::PLATFORM_TARGETS;
use crate::published_versions::PublishedVersions;
use anyhow::Context;
use anyhow::Result;
use std::fmt::Write as _;
use std::fs;

pub(crate) const MISE_PATH: &str = "mise.toml";

const TOOL_PINS_BLOCK: &str = "claude = \"2.1.220\"\n\"github:anomalyco/opencode\" = \"1.17.4\"";

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedBinarySpec {
    tool_name: &'static str,
    version_prefix: &'static str,
    version: String,
}

pub fn render_tool_pins() -> &'static str {
    TOOL_PINS_BLOCK
}

fn resolve_binary_specs(published: &PublishedVersions) -> Result<Vec<ResolvedBinarySpec>> {
    BINARY_SPECS
        .iter()
        .map(|spec| {
            Ok(ResolvedBinarySpec {
                tool_name: spec.tool_name,
                version_prefix: spec.version_prefix,
                version: published.version_for(spec.tool_name)?.to_string(),
            })
        })
        .collect()
}

fn render_agentic_binaries_from_specs(specs: &[ResolvedBinarySpec]) -> String {
    let mut out = String::new();

    for (index, spec) in specs.iter().enumerate() {
        let tool_name = spec.tool_name;
        let version = &spec.version;
        let version_prefix = spec.version_prefix;

        let _ = writeln!(out, "[tools.{tool_name}]");
        let _ = writeln!(out, "version = \"{version}\"");
        let _ = writeln!(out, "version_prefix = \"{version_prefix}\"");
        out.push('\n');

        let _ = writeln!(out, "[tools.{tool_name}.platforms]");
        for (platform, target) in PLATFORM_TARGETS {
            let _ = writeln!(
                out,
                "{platform} = {{ asset_pattern = \"{tool_name}-{target}.tar.xz\" }}"
            );
        }

        if index + 1 != specs.len() {
            out.push('\n');
        }
    }

    out
}

pub fn render_agentic_binaries() -> Result<String> {
    let published = PublishedVersions::load()?;
    let specs = resolve_binary_specs(&published)?;
    Ok(render_agentic_binaries_from_specs(&specs))
}

fn render_updated_mise(original: &str) -> Result<(String, bool)> {
    let (updated, tool_pins_changed) =
        replace_named_block_toml(original, "mise:tool-pins", render_tool_pins())?;
    let binaries = render_agentic_binaries()?;
    let (updated, agentic_binaries_changed) =
        replace_named_block_toml(&updated, "mise:agentic-binaries", &binaries)?;

    Ok((updated, tool_pins_changed || agentic_binaries_changed))
}

pub fn sync_mise(path: &str, dry_run: bool, check: bool) -> Result<bool> {
    let original = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;
    let (updated, changed) = render_updated_mise(&original)?;

    if changed {
        if check {
            anyhow::bail!(
                "mise.toml managed blocks are out of date; run `cargo run -p xtask -- sync`"
            );
        }

        if dry_run {
            eprintln!("[sync] Would update {path} (dry-run)");
        } else {
            fs::write(path, &updated).with_context(|| format!("Failed to write {path}"))?;
            eprintln!("[sync] Updated {path}");
        }
    }

    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_agentic_binaries() -> &'static str {
        r#"[tools.thoughts-bin]
version = "0.1.24"
version_prefix = "thoughts-bin-v"

[tools.thoughts-bin.platforms]
linux-x64 = { asset_pattern = "thoughts-bin-x86_64-unknown-linux-gnu.tar.xz" }
linux-arm64 = { asset_pattern = "thoughts-bin-aarch64-unknown-linux-gnu.tar.xz" }
macos-x64 = { asset_pattern = "thoughts-bin-x86_64-apple-darwin.tar.xz" }
macos-arm64 = { asset_pattern = "thoughts-bin-aarch64-apple-darwin.tar.xz" }

[tools.agentic-bin]
version = "0.1.15"
version_prefix = "agentic-bin-v"

[tools.agentic-bin.platforms]
linux-x64 = { asset_pattern = "agentic-bin-x86_64-unknown-linux-gnu.tar.xz" }
linux-arm64 = { asset_pattern = "agentic-bin-aarch64-unknown-linux-gnu.tar.xz" }
macos-x64 = { asset_pattern = "agentic-bin-x86_64-apple-darwin.tar.xz" }
macos-arm64 = { asset_pattern = "agentic-bin-aarch64-apple-darwin.tar.xz" }

[tools.agentic-mcp]
version = "0.2.42"
version_prefix = "agentic-mcp-v"

[tools.agentic-mcp.platforms]
linux-x64 = { asset_pattern = "agentic-mcp-x86_64-unknown-linux-gnu.tar.xz" }
linux-arm64 = { asset_pattern = "agentic-mcp-aarch64-unknown-linux-gnu.tar.xz" }
macos-x64 = { asset_pattern = "agentic-mcp-x86_64-apple-darwin.tar.xz" }
macos-arm64 = { asset_pattern = "agentic-mcp-aarch64-apple-darwin.tar.xz" }

[tools.agentic-outer-dag-bin]
version = "0.1.0"
version_prefix = "agentic-outer-dag-bin-v"

[tools.agentic-outer-dag-bin.platforms]
linux-x64 = { asset_pattern = "agentic-outer-dag-bin-x86_64-unknown-linux-gnu.tar.xz" }
linux-arm64 = { asset_pattern = "agentic-outer-dag-bin-aarch64-unknown-linux-gnu.tar.xz" }
macos-x64 = { asset_pattern = "agentic-outer-dag-bin-x86_64-apple-darwin.tar.xz" }
macos-arm64 = { asset_pattern = "agentic-outer-dag-bin-aarch64-apple-darwin.tar.xz" }

[tools.opencode-orchestrator-mcp]
version = "0.7.9"
version_prefix = "opencode-orchestrator-mcp-v"

[tools.opencode-orchestrator-mcp.platforms]
linux-x64 = { asset_pattern = "opencode-orchestrator-mcp-x86_64-unknown-linux-gnu.tar.xz" }
linux-arm64 = { asset_pattern = "opencode-orchestrator-mcp-aarch64-unknown-linux-gnu.tar.xz" }
macos-x64 = { asset_pattern = "opencode-orchestrator-mcp-x86_64-apple-darwin.tar.xz" }
macos-arm64 = { asset_pattern = "opencode-orchestrator-mcp-aarch64-apple-darwin.tar.xz" }
"#
    }

    fn sample_resolved_specs() -> Vec<ResolvedBinarySpec> {
        vec![
            ResolvedBinarySpec {
                tool_name: "thoughts-bin",
                version_prefix: "thoughts-bin-v",
                version: "0.1.24".to_string(),
            },
            ResolvedBinarySpec {
                tool_name: "agentic-bin",
                version_prefix: "agentic-bin-v",
                version: "0.1.15".to_string(),
            },
            ResolvedBinarySpec {
                tool_name: "agentic-mcp",
                version_prefix: "agentic-mcp-v",
                version: "0.2.42".to_string(),
            },
            ResolvedBinarySpec {
                tool_name: "agentic-outer-dag-bin",
                version_prefix: "agentic-outer-dag-bin-v",
                version: "0.1.0".to_string(),
            },
            ResolvedBinarySpec {
                tool_name: "opencode-orchestrator-mcp",
                version_prefix: "opencode-orchestrator-mcp-v",
                version: "0.7.9".to_string(),
            },
        ]
    }

    fn sample_mise() -> &'static str {
        r#"[tool_alias]
thoughts-bin = "github:allisoneer/agentic_auxilary"

[tools]
# BEGIN:xtask:autogen mise:tool-pins
claude = "0.0.0"
# END:xtask:autogen
just = "latest"

# Keep nested per-platform tables at the end so later flat tool entries remain in top-level [tools].
# BEGIN:xtask:autogen mise:agentic-binaries
[tools.thoughts-bin]
version = "0.0.0"
# END:xtask:autogen

[env]
_.path = ["tools/bin"]
"#
    }

    #[test]
    fn renders_tool_pins_exactly() {
        assert_eq!(render_tool_pins(), TOOL_PINS_BLOCK);
    }

    #[test]
    fn renders_agentic_binaries_exactly() {
        let rendered = render_agentic_binaries_from_specs(&sample_resolved_specs());
        assert_eq!(rendered, expected_agentic_binaries());
    }

    #[test]
    fn replacing_blocks_preserves_human_owned_toml() {
        let (updated, changed) =
            render_updated_mise_with_specs(sample_mise(), &sample_resolved_specs())
                .expect("render updated mise");

        assert!(changed);
        assert!(
            updated.contains("[tool_alias]\nthoughts-bin = \"github:allisoneer/agentic_auxilary\"")
        );
        assert!(updated.contains("just = \"latest\""));
        assert!(updated.contains("[env]\n_.path = [\"tools/bin\"]"));
        assert!(updated.contains(render_tool_pins()));
        assert!(updated.contains(expected_agentic_binaries()));
    }

    #[test]
    fn block_replacement_is_idempotent() {
        let (first, changed_first) =
            render_updated_mise_with_specs(sample_mise(), &sample_resolved_specs())
                .expect("first render");
        let (second, changed_second) =
            render_updated_mise_with_specs(&first, &sample_resolved_specs())
                .expect("second render");

        assert!(changed_first);
        assert!(!changed_second);
        assert_eq!(first, second);
    }

    fn render_updated_mise_with_specs(
        original: &str,
        specs: &[ResolvedBinarySpec],
    ) -> Result<(String, bool)> {
        let (updated, tool_pins_changed) =
            replace_named_block_toml(original, "mise:tool-pins", render_tool_pins())?;
        let binaries = render_agentic_binaries_from_specs(specs);
        let (updated, agentic_binaries_changed) =
            replace_named_block_toml(&updated, "mise:agentic-binaries", &binaries)?;
        Ok((updated, tool_pins_changed || agentic_binaries_changed))
    }

    #[test]
    fn sync_target_constant_is_root_mise_toml() {
        assert_eq!(MISE_PATH, "mise.toml");
    }

    #[test]
    fn resolves_binary_specs_from_published_versions() {
        let published = PublishedVersions::parse(
            r#"schema_version = 1

[binaries]
agentic-bin = "0.1.15"
agentic-mcp = "0.2.42"
agentic-outer-dag-bin = "0.1.0"
opencode-orchestrator-mcp = "0.7.9"
thoughts-bin = "0.1.24"
"#,
        )
        .expect("published versions parse");

        assert_eq!(
            resolve_binary_specs(&published).expect("resolve"),
            sample_resolved_specs()
        );
    }
}
