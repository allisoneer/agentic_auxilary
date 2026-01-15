//! CLAUDE.md generation utilities.
//!
//! Generates root and per-crate CLAUDE.md content from workspace metadata.

use crate::autogen::replace_named_block;
use anyhow::{Context, Result};
use cargo_metadata::{Metadata, Package};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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

/// Extract metadata.repo.integrations.<key> as bool.
fn get_integration(pkg: &Package, key: &str) -> bool {
    pkg.metadata
        .get("repo")
        .and_then(|r| r.get("integrations"))
        .and_then(|i| i.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Convert a directory path to a workspace-relative string.
///
/// Returns repo-relative paths (e.g., "crates/foo") with forward slashes.
/// Returns "." for the workspace root itself.
fn workspace_relative_dir(dir: &Path, ws_root: &Path) -> String {
    let rel = dir.strip_prefix(ws_root).unwrap_or(dir);
    let s = rel.to_string_lossy().replace('\\', "/");
    let s = s.trim_start_matches('/').to_string();
    if s.is_empty() { ".".to_string() } else { s }
}

/// Render the crate index for root CLAUDE.md, grouped by family.
pub fn render_root_index(metadata: &Metadata) -> String {
    // Group packages by family
    let mut groups: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }

        let family = get_family(pkg).to_string();
        let role = get_role(pkg).to_string();

        // Get relative path from manifest
        let manifest_path = pkg.manifest_path.as_std_path();
        let dir = manifest_path
            .parent()
            .map(|p| workspace_relative_dir(p, metadata.workspace_root.as_std_path()))
            .unwrap_or_default();

        groups
            .entry(family)
            .or_default()
            .push((pkg.name.clone(), role, dir));
    }

    // Render as markdown list
    let mut out = String::new();
    for (family, items) in &groups {
        out.push_str(&format!("### {family}\n"));
        for (name, role, dir) in items {
            out.push_str(&format!("- `{name}` ({role}) - `{dir}/`\n"));
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// Sync root CLAUDE.md crate-index block.
///
/// Returns true if the file was changed.
pub fn sync_root_claude(
    path: &str,
    metadata: &Metadata,
    dry_run: bool,
    check: bool,
) -> Result<bool> {
    let original = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;

    let body = render_root_index(metadata);
    let (updated, changed) = replace_named_block(&original, "crate-index", &body)?;

    if changed {
        if check {
            anyhow::bail!(
                "Root CLAUDE.md crate-index is out of date; run `cargo run -p xtask -- sync`"
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

/// Sync per-crate CLAUDE.md files.
///
/// Creates new files from template or updates existing autogen blocks.
/// Returns the number of files changed.
pub fn sync_crate_claude_files(
    metadata: &Metadata,
    template: &str,
    dry_run: bool,
    check: bool,
) -> Result<usize> {
    let mut changed_count = 0;
    let ws_root = metadata.workspace_root.as_std_path();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }

        let manifest_path = pkg.manifest_path.as_std_path();
        let dir = manifest_path
            .parent()
            .context("Package has no parent dir")?;
        let claude_path = dir.join("CLAUDE.md");

        let changed =
            sync_single_crate_claude(pkg, ws_root, &claude_path, template, dry_run, check)?;
        if changed {
            changed_count += 1;
        }
    }

    Ok(changed_count)
}

/// Sync a single crate's CLAUDE.md file.
fn sync_single_crate_claude(
    pkg: &Package,
    ws_root: &Path,
    path: &Path,
    template: &str,
    dry_run: bool,
    check: bool,
) -> Result<bool> {
    let role = get_role(pkg);
    let family = get_family(pkg);
    let mcp = get_integration(pkg, "mcp");
    let logging = get_integration(pkg, "logging");
    let napi = get_integration(pkg, "napi");

    // Get crate directory relative path
    let manifest_path = pkg.manifest_path.as_std_path();
    let crate_dir = manifest_path
        .parent()
        .map(|p| workspace_relative_dir(p, ws_root))
        .unwrap_or_default();

    // Build header content
    let header = format!(
        "- Crate: {}\n- Path: {}/\n- Role: {}\n- Family: {}\n- Integrations: mcp={}, logging={}, napi={}",
        pkg.name, crate_dir, role, family, mcp, logging, napi
    );

    // Build commands content
    let commands = format!(
        r#"```bash
# Lint & Clippy
cargo fmt -p {} -- --check
cargo clippy -p {} --all-targets -- -D warnings

# Tests
cargo test -p {}

# Build
cargo build -p {}
```"#,
        pkg.name, pkg.name, pkg.name, pkg.name
    );

    // Read or create file
    let original = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?
    } else {
        // Create from template
        template
            .replace("{{crate.name}}", &pkg.name)
            .replace("{{crate.path}}", &crate_dir)
    };

    // Replace autogen blocks
    let (after_header, h_changed) = replace_named_block(&original, "header", &header)?;
    let (after_cmds, c_changed) = replace_named_block(&after_header, "commands", &commands)?;

    let changed = h_changed || c_changed || !path.exists();

    if changed {
        if check {
            anyhow::bail!(
                "{} CLAUDE.md is out of date; run `cargo run -p xtask -- sync`",
                pkg.name
            );
        }
        if !dry_run {
            fs::write(path, &after_cmds)
                .with_context(|| format!("Failed to write {}", path.display()))?;
            eprintln!("[sync] Updated {}", path.display());
        } else {
            eprintln!("[sync] Would update {} (dry-run)", path.display());
        }
    }

    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_workspace_relative_dir_basic() {
        let ws = Path::new("/repo");
        let dir = Path::new("/repo/crates/foo");
        let rel = workspace_relative_dir(dir, ws);
        assert!(!rel.starts_with('/'), "should be repo-relative");
        assert_eq!(rel, "crates/foo");
    }

    #[test]
    fn test_workspace_relative_dir_root() {
        let ws = Path::new("/repo");
        let dir = Path::new("/repo");
        let rel = workspace_relative_dir(dir, ws);
        assert_eq!(rel, ".", "root should map to '.'");
    }

    #[test]
    fn test_workspace_relative_dir_nested() {
        let ws = Path::new("/home/user/project");
        let dir = Path::new("/home/user/project/crates/tools/my-tool");
        let rel = workspace_relative_dir(dir, ws);
        assert_eq!(rel, "crates/tools/my-tool");
    }
}
