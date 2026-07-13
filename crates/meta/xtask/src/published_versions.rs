//! Repo-owned published-version contract helpers.

use crate::managed_mise::BINARY_SPECS;
use crate::managed_mise::binary_spec;
use crate::managed_mise::required_asset_names;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use cargo_metadata::semver::Version;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;

pub const PUBLISHED_VERSIONS_PATH: &str = "tools/published-versions.toml";
const SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedVersions {
    binaries: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedTag {
    pub tool_name: String,
    pub version: String,
    pub tag: String,
    pub is_prerelease: bool,
    pub required_asset_names: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishedVersionsOutcome {
    pub managed: bool,
    pub tool_name: Option<String>,
    pub version: Option<String>,
    pub tag: String,
    pub is_prerelease: bool,
    pub changed: bool,
    pub required_asset_names: Vec<String>,
}

#[derive(Deserialize)]
struct PublishedVersionsFile {
    schema_version: u64,
    binaries: BTreeMap<String, String>,
}

impl PublishedVersions {
    pub fn load() -> Result<Self> {
        Self::load_from(PUBLISHED_VERSIONS_PATH)
    }

    pub fn load_from(path: &str) -> Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;
        Self::parse(&contents).with_context(|| format!("Failed to parse {path}"))
    }

    pub fn parse(contents: &str) -> Result<Self> {
        let parsed: PublishedVersionsFile = toml::from_str(contents)?;
        if parsed.schema_version != SCHEMA_VERSION {
            bail!(
                "Unsupported schema_version {} in published-versions contract; expected {SCHEMA_VERSION}",
                parsed.schema_version
            );
        }

        let expected: BTreeMap<String, String> = BINARY_SPECS
            .iter()
            .map(|spec| (spec.tool_name.to_string(), spec.version_prefix.to_string()))
            .collect();

        for tool_name in parsed.binaries.keys() {
            if !expected.contains_key(tool_name) {
                bail!("Unknown managed binary `{tool_name}` in published-versions contract");
            }
        }

        for tool_name in expected.keys() {
            if !parsed.binaries.contains_key(tool_name) {
                bail!(
                    "Missing required managed binary `{tool_name}` in published-versions contract"
                );
            }
        }

        for (tool_name, version) in &parsed.binaries {
            Version::parse(version).with_context(|| {
                format!("Invalid semver `{version}` for managed binary `{tool_name}`")
            })?;
        }

        Ok(Self {
            binaries: parsed.binaries,
        })
    }

    pub fn version_for(&self, tool_name: &str) -> Result<&str> {
        self.binaries
            .get(tool_name)
            .map(String::as_str)
            .with_context(|| format!("Missing published version for managed binary `{tool_name}`"))
    }

    pub fn set_monotonic(&mut self, tool_name: &str, version: &str) -> Result<bool> {
        if binary_spec(tool_name).is_none() {
            bail!("Unknown managed binary `{tool_name}`");
        }

        let new_version = Version::parse(version).with_context(|| {
            format!("Invalid semver `{version}` for managed binary `{tool_name}`")
        })?;
        let current = self.version_for(tool_name)?;
        let current_version = Version::parse(current).with_context(|| {
            format!("Invalid existing semver `{current}` for managed binary `{tool_name}`")
        })?;

        if new_version < current_version {
            bail!("Refusing to downgrade managed binary `{tool_name}` from {current} to {version}");
        }
        if new_version == current_version {
            return Ok(false);
        }

        self.binaries
            .insert(tool_name.to_string(), version.to_string());
        Ok(true)
    }

    pub fn write_canonical(&self, path: &str) -> Result<()> {
        fs::write(path, self.to_canonical_toml()).with_context(|| format!("Failed to write {path}"))
    }

    pub fn to_canonical_toml(&self) -> String {
        let mut out = format!("schema_version = {SCHEMA_VERSION}\n\n[binaries]\n");
        for (tool_name, version) in &self.binaries {
            let _ = writeln!(out, "{tool_name} = \"{version}\"");
        }
        out
    }
}

pub fn parse_managed_tag(tag: &str) -> Result<Option<ParsedTag>> {
    for spec in &BINARY_SPECS {
        let Some(version) = tag.strip_prefix(spec.version_prefix) else {
            continue;
        };
        let parsed = Version::parse(version)
            .with_context(|| format!("Tag `{tag}` has invalid semver version `{version}`"))?;
        return Ok(Some(ParsedTag {
            tool_name: spec.tool_name.to_string(),
            version: version.to_string(),
            tag: tag.to_string(),
            is_prerelease: !parsed.pre.is_empty(),
            required_asset_names: required_asset_names(spec.tool_name),
        }));
    }

    Ok(None)
}

pub fn apply_tag_update(tag: &str, dry_run: bool) -> Result<PublishedVersionsOutcome> {
    let Some(parsed) = parse_managed_tag(tag)? else {
        return Ok(PublishedVersionsOutcome {
            managed: false,
            tool_name: None,
            version: None,
            tag: tag.to_string(),
            is_prerelease: false,
            changed: false,
            required_asset_names: Vec::new(),
        });
    };

    let mut published = PublishedVersions::load()?;
    let changed = if parsed.is_prerelease {
        false
    } else {
        let changed = published.set_monotonic(&parsed.tool_name, &parsed.version)?;
        if changed && !dry_run {
            published.write_canonical(PUBLISHED_VERSIONS_PATH)?;
        }
        changed
    };

    Ok(PublishedVersionsOutcome {
        managed: true,
        tool_name: Some(parsed.tool_name),
        version: Some(parsed.version),
        tag: parsed.tag,
        is_prerelease: parsed.is_prerelease,
        changed,
        required_asset_names: parsed.required_asset_names,
    })
}

pub fn run(tag: &str, dry_run: bool, json: bool) -> Result<()> {
    let outcome = apply_tag_update(tag, dry_run)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
        return Ok(());
    }

    if !outcome.managed {
        eprintln!("[published-versions] Tag `{tag}` is unmanaged; no-op.");
        return Ok(());
    }

    if outcome.is_prerelease {
        eprintln!("[published-versions] Tag `{tag}` is a prerelease; no-op.");
        return Ok(());
    }

    if outcome.changed {
        if dry_run {
            eprintln!(
                "[published-versions] Would update {PUBLISHED_VERSIONS_PATH} from tag `{tag}`."
            );
        } else {
            eprintln!("[published-versions] Updated {PUBLISHED_VERSIONS_PATH} from tag `{tag}`.");
        }
    } else {
        eprintln!("[published-versions] No change needed for tag `{tag}`.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contract() -> &'static str {
        r#"schema_version = 1

[binaries]
agentic-bin = "0.1.15"
agentic-mcp = "0.2.42"
agentic-outer-dag-bin = "0.1.0"
opencode-orchestrator-mcp = "0.7.9"
thoughts-bin = "0.1.24"
"#
    }

    #[test]
    fn loads_valid_contract() {
        let parsed = PublishedVersions::parse(sample_contract()).expect("valid contract");
        assert_eq!(
            parsed.version_for("agentic-bin").expect("version"),
            "0.1.15"
        );
    }

    #[test]
    fn rejects_missing_required_key() {
        let err = PublishedVersions::parse(
            r#"schema_version = 1

[binaries]
agentic-bin = "0.1.15"
agentic-mcp = "0.2.42"
opencode-orchestrator-mcp = "0.7.9"
"#,
        )
        .expect_err("missing key should fail")
        .to_string();
        assert!(err.contains("Missing required managed binary `agentic-outer-dag-bin`"));
    }

    #[test]
    fn rejects_unknown_key() {
        let err = PublishedVersions::parse(
            r#"schema_version = 1

[binaries]
agentic-bin = "0.1.15"
agentic-mcp = "0.2.42"
message-optimizer-bin = "0.1.0"
opencode-orchestrator-mcp = "0.7.9"
thoughts-bin = "0.1.24"
"#,
        )
        .expect_err("unknown key should fail")
        .to_string();
        assert!(err.contains("Unknown managed binary `message-optimizer-bin`"));
    }

    #[test]
    fn rejects_invalid_semver() {
        let err = PublishedVersions::parse(
            r#"schema_version = 1

[binaries]
agentic-bin = "not-semver"
agentic-mcp = "0.2.42"
agentic-outer-dag-bin = "0.1.0"
opencode-orchestrator-mcp = "0.7.9"
thoughts-bin = "0.1.24"
"#,
        )
        .expect_err("invalid semver should fail")
        .to_string();
        assert!(err.contains("Invalid semver `not-semver` for managed binary `agentic-bin`"));
    }

    #[test]
    fn recognizes_managed_and_unmanaged_tags() {
        let parsed = parse_managed_tag("agentic-bin-v0.1.15")
            .expect("tag parse")
            .expect("managed tag");
        assert_eq!(parsed.tool_name, "agentic-bin");
        assert_eq!(parsed.version, "0.1.15");
        assert!(!parsed.is_prerelease);
        assert_eq!(
            parsed.required_asset_names,
            vec![
                "agentic-bin-x86_64-unknown-linux-gnu.tar.xz",
                "agentic-bin-aarch64-unknown-linux-gnu.tar.xz",
                "agentic-bin-x86_64-apple-darwin.tar.xz",
                "agentic-bin-aarch64-apple-darwin.tar.xz",
            ]
        );

        assert_eq!(
            parse_managed_tag("message-optimizer-bin-v0.1.0").expect("parse"),
            None
        );

        let outer_dag = parse_managed_tag("agentic-outer-dag-bin-v0.1.0")
            .expect("tag parse")
            .expect("managed tag");
        assert_eq!(outer_dag.tool_name, "agentic-outer-dag-bin");
        assert_eq!(outer_dag.version, "0.1.0");
        assert_eq!(
            outer_dag.required_asset_names,
            vec![
                "agentic-outer-dag-bin-x86_64-unknown-linux-gnu.tar.xz",
                "agentic-outer-dag-bin-aarch64-unknown-linux-gnu.tar.xz",
                "agentic-outer-dag-bin-x86_64-apple-darwin.tar.xz",
                "agentic-outer-dag-bin-aarch64-apple-darwin.tar.xz",
            ]
        );
    }

    #[test]
    fn marks_prerelease_tags() {
        let parsed = parse_managed_tag("agentic-mcp-v0.2.42-rc.1")
            .expect("tag parse")
            .expect("managed tag");
        assert!(parsed.is_prerelease);
    }

    #[test]
    fn unmanaged_tag_outcome_is_noop() {
        let outcome = apply_tag_update("message-optimizer-bin-v0.1.0", true).expect("outcome");
        assert!(!outcome.managed);
        assert!(!outcome.changed);
    }

    #[test]
    fn refuses_monotonic_downgrade() {
        let mut published = PublishedVersions::parse(sample_contract()).expect("valid contract");
        let err = published
            .set_monotonic("agentic-bin", "0.1.14")
            .expect_err("downgrade should fail")
            .to_string();
        assert!(err.contains("Refusing to downgrade managed binary `agentic-bin`"));
    }

    #[test]
    fn canonical_writer_is_sorted_and_round_trips() {
        let published = PublishedVersions::parse(sample_contract()).expect("valid contract");
        let rendered = published.to_canonical_toml();
        assert_eq!(
            PublishedVersions::parse(&rendered).expect("round trip"),
            published
        );
        assert!(rendered.contains("schema_version = 1\n\n[binaries]\nagentic-bin = \"0.1.15\"\n"));
    }
}
