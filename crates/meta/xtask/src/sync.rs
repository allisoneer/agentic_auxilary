//! Sync command implementation.
//!
//! Updates autogen blocks in CLAUDE.md files, release-plz.toml, and README.md.

use crate::{claude, policy::Policy, readme, release_plz};
use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use std::fs;

/// Run the sync command.
///
/// Updates:
/// - Root CLAUDE.md crate-index
/// - Per-crate CLAUDE.md files
/// - release-plz.toml packages block
pub fn run(dry_run: bool, check: bool) -> Result<()> {
    eprintln!("[sync] Loading workspace metadata...");
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run `cargo metadata`")?;

    eprintln!("[sync] Loading policy from tools/policy.toml...");
    let policy = Policy::load()?;

    // Root CLAUDE.md
    eprintln!("[sync] Syncing root CLAUDE.md...");
    let root_changed = claude::sync_root_claude("CLAUDE.md", &metadata, dry_run, check)?;

    // Per-crate CLAUDE.md files
    eprintln!("[sync] Syncing per-crate CLAUDE.md files...");
    let template_path = "tools/templates/CLAUDE.template.md";
    let template = fs::read_to_string(template_path)
        .with_context(|| format!("Failed to read template at {template_path}"))?;
    let crate_count = claude::sync_crate_claude_files(&metadata, &template, dry_run, check)?;

    // release-plz.toml
    eprintln!("[sync] Syncing release-plz.toml...");
    let release_changed =
        release_plz::sync_release_plz("release-plz.toml", &metadata, &policy, dry_run, check)?;

    // README.md
    eprintln!("[sync] Syncing README.md...");
    let readme_changed = readme::sync_root_readme("README.md", &metadata, dry_run, check)?;

    // Summary
    let total_changes = (root_changed as usize)
        + crate_count
        + (release_changed as usize)
        + (readme_changed as usize);
    if total_changes == 0 {
        eprintln!("[sync] No changes needed.");
    } else if dry_run {
        eprintln!("[sync] Would make {} change(s) (dry-run).", total_changes);
    } else {
        eprintln!("[sync] Made {} change(s).", total_changes);
    }

    Ok(())
}
