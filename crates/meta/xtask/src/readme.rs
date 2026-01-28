//! README.md auto-generation utilities.
//!
//! Generates structured crate listings for the root README from workspace metadata.
//! Supports hybrid tiering: role-based defaults with optional `readme_tier` overrides.

use crate::autogen::replace_named_block;
use cargo_metadata::{Metadata, Package};
use std::collections::BTreeMap;
use std::path::Path;

/// README tier determines how a crate appears in the README.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadmeTier {
    /// Rich manual prose in Primary Tools section (not auto-generated).
    Featured,
    /// Auto-generated with description in Additional Tools section.
    Brief,
    /// Auto-generated minimal in Supporting Libraries section.
    OneLiner,
    /// Auto-generated in Legacy section.
    Legacy,
    /// Not shown in README.
    Omit,
}

/// Information about a crate for README rendering.
#[derive(Debug)]
pub struct CrateInfo {
    pub name: String,
    pub family: String,
    pub description: String,
    pub dir: String,
    pub tier: ReadmeTier,
}

/// Extract metadata.repo.role from a package, defaulting to "unknown".
fn get_role(pkg: &Package) -> &str {
    pkg.metadata
        .get("repo")
        .and_then(|r| r.get("role"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
}

/// Extract metadata.repo.family from a package, defaulting to "unknown".
fn get_family(pkg: &Package) -> &str {
    pkg.metadata
        .get("repo")
        .and_then(|r| r.get("family"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
}

/// Extract metadata.repo.readme_tier from a package, if present.
fn get_readme_tier_override(pkg: &Package) -> Option<&str> {
    pkg.metadata
        .get("repo")
        .and_then(|r| r.get("readme_tier"))
        .and_then(|v| v.as_str())
}

/// Get the default tier for a given role.
pub fn default_tier_for_role(role: &str) -> ReadmeTier {
    match role {
        "app" => ReadmeTier::Featured,
        "tool-lib" => ReadmeTier::Brief,
        "lib" | "binding" => ReadmeTier::OneLiner,
        "legacy" => ReadmeTier::Legacy,
        "xtask" => ReadmeTier::Omit,
        _ => ReadmeTier::OneLiner,
    }
}

/// Parse a tier string into a ReadmeTier.
fn parse_tier(s: &str) -> Option<ReadmeTier> {
    match s {
        "featured" => Some(ReadmeTier::Featured),
        "brief" => Some(ReadmeTier::Brief),
        "one-liner" => Some(ReadmeTier::OneLiner),
        "legacy" => Some(ReadmeTier::Legacy),
        "omit" => Some(ReadmeTier::Omit),
        _ => None,
    }
}

/// Determine the tier for a package: override if present, else default from role.
pub fn tier_for(pkg: &Package) -> ReadmeTier {
    if let Some(override_str) = get_readme_tier_override(pkg)
        && let Some(tier) = parse_tier(override_str)
    {
        return tier;
    }
    default_tier_for_role(get_role(pkg))
}

/// Convert a directory path to a workspace-relative string.
fn workspace_relative_dir(dir: &Path, ws_root: &Path) -> String {
    let rel = dir.strip_prefix(ws_root).unwrap_or(dir);
    let s = rel.to_string_lossy().replace('\\', "/");
    let s = s.trim_start_matches('/').to_string();
    if s.is_empty() { ".".to_string() } else { s }
}

/// Collect crate information from workspace metadata.
pub fn collect_crates(metadata: &Metadata) -> Vec<CrateInfo> {
    let ws_root = metadata.workspace_root.as_std_path();
    let mut crates = Vec::new();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }

        let tier = tier_for(pkg);
        let family = get_family(pkg).to_string();
        let description = pkg.description.clone().unwrap_or_else(|| pkg.name.clone());

        let manifest_path = pkg.manifest_path.as_std_path();
        let dir = manifest_path
            .parent()
            .map(|p| workspace_relative_dir(p, ws_root))
            .unwrap_or_default();

        crates.push(CrateInfo {
            name: pkg.name.clone(),
            family,
            description,
            dir,
            tier,
        });
    }

    crates
}

/// Render a grouped list of crates for a specific tier.
///
/// Groups by family (BTreeMap for stable ordering), sorts by name within groups.
pub fn render_grouped_list(crates: &[CrateInfo], tier: ReadmeTier) -> String {
    // Filter to the requested tier
    let mut filtered: Vec<&CrateInfo> = crates.iter().filter(|c| c.tier == tier).collect();

    // Sort by (family, name) for stable output
    filtered.sort_by(|a, b| (&a.family, &a.name).cmp(&(&b.family, &b.name)));

    // Group by family
    let mut groups: BTreeMap<&str, Vec<&CrateInfo>> = BTreeMap::new();
    for c in filtered {
        groups.entry(&c.family).or_default().push(c);
    }

    // Render
    let mut out = String::new();
    for (family, items) in &groups {
        out.push_str(&format!("### {family}\n\n"));
        for c in items {
            out.push_str(&format!(
                "- [`{}`]({}) - {}\n",
                c.name, c.dir, c.description
            ));
        }
        out.push('\n');
    }

    out.trim_end().to_string()
}

/// Apply README autogen blocks for the three auto-generated sections.
///
/// Block keys:
/// - `readme-additional-tools` (Brief tier)
/// - `readme-supporting-libraries` (OneLiner tier)
/// - `readme-legacy` (Legacy tier)
pub fn apply_readme_blocks(input: &str, crates: &[CrateInfo]) -> anyhow::Result<(String, bool)> {
    let additional = render_grouped_list(crates, ReadmeTier::Brief);
    let supporting = render_grouped_list(crates, ReadmeTier::OneLiner);
    let legacy = render_grouped_list(crates, ReadmeTier::Legacy);

    let (after_additional, c1) =
        replace_named_block(input, "readme-additional-tools", &additional)?;
    let (after_supporting, c2) = replace_named_block(
        &after_additional,
        "readme-supporting-libraries",
        &supporting,
    )?;
    let (after_legacy, c3) = replace_named_block(&after_supporting, "readme-legacy", &legacy)?;

    Ok((after_legacy, c1 || c2 || c3))
}

/// Render the full README content with autogen blocks and autodeps markers applied.
///
/// This first applies the autogen blocks for crate listings, then applies
/// autodeps markers for version pinning.
pub fn render_root_readme(
    input: &str,
    metadata: &cargo_metadata::Metadata,
    strict: bool,
) -> anyhow::Result<(String, bool)> {
    let crates = collect_crates(metadata);

    // Apply autogen blocks first
    let (after_autogen, autogen_changed) = apply_readme_blocks(input, &crates)?;

    // Then apply autodeps markers
    let (output, autodeps_changed) = crate::marker::apply_autodeps_markers(
        &after_autogen,
        &crate::marker::RenderContext { metadata, strict },
    )?;

    Ok((output, autogen_changed || autodeps_changed))
}

/// Sync the root README.md file.
///
/// This is the internal implementation used by `xtask sync`.
/// Does NOT print to stdout on dry-run (unlike CLI version).
pub fn sync_root_readme(
    path: &str,
    metadata: &cargo_metadata::Metadata,
    dry_run: bool,
    check: bool,
) -> anyhow::Result<bool> {
    let original =
        std::fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;

    let (output, changed) = render_root_readme(&original, metadata, false)?;

    if !changed {
        return Ok(false);
    }

    if check {
        anyhow::bail!("README.md is out of date; run `cargo run -p xtask -- sync`");
    }

    if !dry_run {
        std::fs::write(path, &output).with_context(|| format!("Failed to write {path}"))?;
        eprintln!("[sync] Updated {path}");
    } else {
        eprintln!("[sync] Would update {path} (dry-run)");
    }

    Ok(true)
}

/// Sync the root README.md file with CLI-friendly output.
///
/// This is used by `xtask readme-sync` and prints to stdout on dry-run.
pub fn sync_root_readme_cli(
    path: &std::path::Path,
    metadata: &cargo_metadata::Metadata,
    dry_run: bool,
    check: bool,
    strict: bool,
) -> anyhow::Result<()> {
    let original = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let (output, changed) = render_root_readme(&original, metadata, strict)?;

    if !changed {
        eprintln!("[readme-sync] No changes needed for {}", path.display());
        return Ok(());
    }

    if check {
        anyhow::bail!("README is out of sync. Run `cargo run -p xtask -- readme-sync` to update.");
    }

    if dry_run {
        // CLI behavior: print full content to stdout
        println!("{output}");
    } else {
        std::fs::write(path, &output)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        eprintln!("[readme-sync] Updated {}", path.display());
    }

    Ok(())
}

use anyhow::Context;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tiers_by_role() {
        assert_eq!(default_tier_for_role("app"), ReadmeTier::Featured);
        assert_eq!(default_tier_for_role("tool-lib"), ReadmeTier::Brief);
        assert_eq!(default_tier_for_role("lib"), ReadmeTier::OneLiner);
        assert_eq!(default_tier_for_role("binding"), ReadmeTier::OneLiner);
        assert_eq!(default_tier_for_role("legacy"), ReadmeTier::Legacy);
        assert_eq!(default_tier_for_role("xtask"), ReadmeTier::Omit);
        assert_eq!(default_tier_for_role("unknown"), ReadmeTier::OneLiner);
    }

    #[test]
    fn test_parse_tier() {
        assert_eq!(parse_tier("featured"), Some(ReadmeTier::Featured));
        assert_eq!(parse_tier("brief"), Some(ReadmeTier::Brief));
        assert_eq!(parse_tier("one-liner"), Some(ReadmeTier::OneLiner));
        assert_eq!(parse_tier("legacy"), Some(ReadmeTier::Legacy));
        assert_eq!(parse_tier("omit"), Some(ReadmeTier::Omit));
        assert_eq!(parse_tier("invalid"), None);
    }

    #[test]
    fn test_render_grouped_list_is_sorted_and_grouped() {
        let crates = vec![
            CrateInfo {
                name: "zebra".to_string(),
                family: "tools".to_string(),
                description: "Zebra crate".to_string(),
                dir: "crates/tools/zebra".to_string(),
                tier: ReadmeTier::Brief,
            },
            CrateInfo {
                name: "alpha".to_string(),
                family: "tools".to_string(),
                description: "Alpha crate".to_string(),
                dir: "crates/tools/alpha".to_string(),
                tier: ReadmeTier::Brief,
            },
            CrateInfo {
                name: "beta".to_string(),
                family: "services".to_string(),
                description: "Beta crate".to_string(),
                dir: "crates/services/beta".to_string(),
                tier: ReadmeTier::Brief,
            },
            CrateInfo {
                name: "omitted".to_string(),
                family: "tools".to_string(),
                description: "Should not appear".to_string(),
                dir: "crates/tools/omitted".to_string(),
                tier: ReadmeTier::OneLiner,
            },
        ];

        let rendered = render_grouped_list(&crates, ReadmeTier::Brief);

        // Services should come before tools (alphabetical by family)
        assert!(rendered.contains("### services"));
        assert!(rendered.contains("### tools"));

        let services_pos = rendered.find("### services").unwrap();
        let tools_pos = rendered.find("### tools").unwrap();
        assert!(
            services_pos < tools_pos,
            "services should come before tools"
        );

        // Within tools, alpha should come before zebra
        let alpha_pos = rendered.find("alpha").unwrap();
        let zebra_pos = rendered.find("zebra").unwrap();
        assert!(alpha_pos < zebra_pos, "alpha should come before zebra");

        // OneLiner tier should not appear
        assert!(!rendered.contains("omitted"));
    }

    #[test]
    fn test_apply_readme_blocks_idempotent() {
        let input = r#"# README

## Additional Tools
<!-- BEGIN:xtask:autogen readme-additional-tools -->
old content
<!-- END:xtask:autogen -->

## Supporting Libraries
<!-- BEGIN:xtask:autogen readme-supporting-libraries -->
old supporting
<!-- END:xtask:autogen -->

## Legacy
<!-- BEGIN:xtask:autogen readme-legacy -->
old legacy
<!-- END:xtask:autogen -->
"#;

        let crates = vec![CrateInfo {
            name: "test-tool".to_string(),
            family: "tools".to_string(),
            description: "Test tool".to_string(),
            dir: "crates/tools/test-tool".to_string(),
            tier: ReadmeTier::Brief,
        }];

        let (output, changed) = apply_readme_blocks(input, &crates).unwrap();
        assert!(changed);
        assert!(output.contains("test-tool"));

        // Run again with same input - should be idempotent
        let (output2, changed2) = apply_readme_blocks(&output, &crates).unwrap();
        assert!(!changed2, "should be idempotent");
        assert_eq!(output, output2);
    }
}
