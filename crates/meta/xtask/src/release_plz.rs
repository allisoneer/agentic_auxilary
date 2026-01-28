//! release-plz.toml package entry generation.
//!
//! Generates [[package]] entries from workspace metadata and policy overrides.

use crate::autogen::replace_named_block_toml;
use crate::policy::Policy;
use anyhow::{Context, Result};
use cargo_metadata::Metadata;
use std::fs;

/// Get the changelog path for a package based on its manifest directory.
fn get_changelog_path(pkg: &cargo_metadata::Package, metadata: &Metadata) -> String {
    let manifest_path = pkg.manifest_path.as_std_path();
    let dir = manifest_path.parent().unwrap_or(manifest_path);

    // Make path relative to workspace root
    let ws_root = metadata.workspace_root.as_std_path();
    let relative = dir
        .strip_prefix(ws_root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| dir.to_string_lossy().to_string());

    format!("{}/CHANGELOG.md", relative.trim_start_matches('/'))
}

/// Render [[package]] entries for all workspace crates.
pub fn render_packages(metadata: &Metadata, policy: &Policy) -> String {
    let mut out = String::new();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }

        let name = &pkg.name;
        let changelog_path = get_changelog_path(pkg, metadata);

        // Get overrides from policy
        let override_entry = policy.release_plz.overrides.get(name);

        // Determine publish status
        let publish = override_entry
            .and_then(|o| o.publish)
            .unwrap_or(policy.release_plz.publish_default);

        // Determine git tag status
        let git_tag_enable = override_entry
            .and_then(|o| o.git_tag_enable)
            .unwrap_or(true);

        // Build tag name from format (keep {{ version }} as template placeholder)
        let tag_name = policy
            .release_plz
            .git_tag_format
            .replace("{{ name }}", name);

        out.push_str("[[package]]\n");
        out.push_str(&format!("name = \"{}\"\n", name));
        out.push_str(&format!("changelog_path = \"{}\"\n", changelog_path));
        out.push_str(&format!("git_tag_name = \"{}\"\n", tag_name));
        out.push_str(&format!(
            "git_tag_enable = {}\n",
            if git_tag_enable { "true" } else { "false" }
        ));
        out.push_str(&format!(
            "release = {}\n",
            if publish { "true" } else { "false" }
        ));
        out.push('\n');
    }

    out.trim_end().to_string()
}

/// Sync release-plz.toml packages block.
///
/// Returns true if the file was changed.
pub fn sync_release_plz(
    path: &str,
    metadata: &Metadata,
    policy: &Policy,
    dry_run: bool,
    check: bool,
) -> Result<bool> {
    let original = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;

    let body = render_packages(metadata, policy);
    let (updated, changed) = replace_named_block_toml(&original, "release-plz:packages", &body)?;

    if changed {
        if check {
            anyhow::bail!(
                "release-plz.toml packages block is out of date; run `cargo run -p xtask -- sync`"
            );
        }
        if !dry_run {
            fs::write(path, &updated).with_context(|| format!("Failed to write {path}"))?;
            eprintln!("[sync] Updated {path}");
        } else {
            eprintln!("[sync] Would update {path} (dry-run)");
        }
    }

    Ok(changed)
}
