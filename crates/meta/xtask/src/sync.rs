//! Sync command implementation.
//!
//! Updates autogen blocks in CLAUDE.md files, release-plz.toml, README.md, justfile,
//! mise.toml, and agentic.schema.json.

use crate::claude;
use crate::justfile;
use crate::mise;
use crate::policy::Policy;
use crate::readme;
use crate::release_plz;
use crate::schema;
use anyhow::Context;
use anyhow::Result;
use cargo_metadata::MetadataCommand;
use std::fs;

/// Run the sync command.
///
/// Updates:
/// - Root CLAUDE.md crate-index
/// - Per-crate CLAUDE.md files
/// - release-plz.toml packages block
/// - mise.toml managed blocks
/// - README.md version markers
/// - justfile generated sections
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

    // mise.toml
    eprintln!("[sync] Syncing mise.toml...");
    let mise_changed = mise::sync_mise(mise::MISE_PATH, &metadata, dry_run, check)?;

    // README.md
    eprintln!("[sync] Syncing README.md...");
    let readme_changed = readme::sync_root_readme("README.md", &metadata, dry_run, check)?;

    // justfile
    eprintln!("[sync] Syncing justfile...");
    let justfile_changed = justfile::sync_justfile("justfile", &metadata, dry_run, check)?;

    // agentic.schema.json (6th target)
    // Note: agentic.toml.example is intentionally not auto-synced here and must
    // be updated manually when the modeled config surface changes.
    eprintln!("[sync] Syncing agentic.schema.json...");
    let schema_changed = schema::sync_schema("agentic.schema.json", dry_run, check)?;

    // Summary
    let total_changes = usize::from(root_changed)
        + crate_count
        + usize::from(release_changed)
        + usize::from(mise_changed)
        + usize::from(readme_changed)
        + usize::from(justfile_changed)
        + usize::from(schema_changed);
    if total_changes == 0 {
        eprintln!("[sync] No changes needed.");
    } else if dry_run {
        eprintln!("[sync] Would make {total_changes} change(s) (dry-run).");
    } else {
        eprintln!("[sync] Made {total_changes} change(s).");
    }

    Ok(())
}
