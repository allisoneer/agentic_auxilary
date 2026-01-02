use anyhow::{Context, Result};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::config::{Mount, RepoConfigManager};
use crate::git::utils::{
    find_repo_root, get_control_repo_root, get_current_branch, get_remote_url,
};
use crate::mount::MountResolver;

// Centralized main/master detection
fn is_main_like(branch: &str) -> bool {
    matches!(branch, "main" | "master")
}

// Standardized lockout error text for CLI + MCP
fn main_branch_lockout_error(branch: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "Branch protection: operations that create or access branch-specific work are blocked on '{}'.\n\
         Create a feature branch first, then re-run:\n  git checkout -b my/feature\n\n\
         Note: branch-agnostic commands like 'thoughts work list' and 'thoughts references list' are allowed on main.",
        branch
    )
}

// Detect weekly dir formats "YYYY-WWW" and legacy "YYYY_week_WW"
fn is_weekly_dir_name(name: &str) -> bool {
    // Pattern 1: YYYY-WWW (e.g., "2025-W01")
    if let Some((year, rest)) = name.split_once("-W")
        && year.len() == 4
        && year.chars().all(|c| c.is_ascii_digit())
        && rest.len() == 2
        && rest.chars().all(|c| c.is_ascii_digit())
        && let Ok(w) = rest.parse::<u32>()
    {
        return (1..=53).contains(&w);
    }
    // Pattern 2 (legacy): YYYY_week_WW (e.g., "2025_week_01")
    if let Some((year, rest)) = name.split_once("_week_")
        && year.len() == 4
        && year.chars().all(|c| c.is_ascii_digit())
        && rest.len() == 2
        && rest.chars().all(|c| c.is_ascii_digit())
        && let Ok(w) = rest.parse::<u32>()
    {
        return (1..=53).contains(&w);
    }
    false
}

// Choose collision-free archive name (name, name-migrated, name-migrated-2, ...)
fn next_archive_name(completed_dir: &Path, base_name: &str) -> PathBuf {
    let candidate = completed_dir.join(base_name);
    if !candidate.exists() {
        return candidate;
    }
    let mut i = 1usize;
    loop {
        let with_suffix = if i == 1 {
            format!("{}-migrated", base_name)
        } else {
            format!("{}-migrated-{}", base_name, i)
        };
        let p = completed_dir.join(with_suffix);
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

// Auto-archive weekly dirs from thoughts_root/* -> thoughts_root/completed/*
fn auto_archive_weekly_dirs(thoughts_root: &Path) -> Result<()> {
    let completed = thoughts_root.join("completed");
    std::fs::create_dir_all(&completed).ok();
    for entry in std::fs::read_dir(thoughts_root)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "completed" || name == "active" {
            continue;
        }
        if is_weekly_dir_name(&name) {
            let dest = next_archive_name(&completed, &name);
            debug!("Archiving weekly dir {} -> {}", p.display(), dest.display());
            std::fs::rename(&p, &dest).with_context(|| {
                format!(
                    "Failed to archive weekly dir {} -> {}",
                    p.display(),
                    dest.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Migrate from `thoughts/active/*` structure to `thoughts/*`.
///
/// Moves directories from active/ to the root and creates a compatibility
/// symlink `active -> .` for backward compatibility.
fn migrate_active_layer(thoughts_root: &Path) -> Result<()> {
    let active = thoughts_root.join("active");

    // Check if active is a real directory (not already a symlink)
    if active.exists() && active.is_dir() && !active.is_symlink() {
        debug!("Migrating active/ layer at {}", thoughts_root.display());

        // Move all directories from active/ to thoughts_root
        for entry in std::fs::read_dir(&active)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name();
                let newp = thoughts_root.join(&name);
                if !newp.exists() {
                    std::fs::rename(&p, &newp).with_context(|| {
                        format!("Failed to move {} to {}", p.display(), newp.display())
                    })?;
                    debug!("Migrated {} -> {}", p.display(), newp.display());
                }
            }
        }

        // Create compatibility symlink active -> .
        #[cfg(unix)]
        {
            use std::os::unix::fs as unixfs;
            // Only remove if it's now empty
            if std::fs::read_dir(&active)?.next().is_none() {
                let _ = std::fs::remove_dir(&active);
                if unixfs::symlink(".", &active).is_ok() {
                    debug!("Created compatibility symlink: active -> .");
                }
            }
        }
    }
    Ok(())
}

/// Paths for the current active work directory
#[derive(Debug, Clone)]
pub struct ActiveWork {
    pub dir_name: String,
    pub base: PathBuf,
    pub research: PathBuf,
    pub plans: PathBuf,
    pub artifacts: PathBuf,
}

/// Resolve thoughts root via configured thoughts_mount
fn resolve_thoughts_root() -> Result<PathBuf> {
    let control_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(control_root);
    let ds = mgr.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    let tm = ds.thoughts_mount.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "No thoughts_mount configured in repository configuration.\n\
             Add thoughts_mount to .thoughts/config.json and run 'thoughts mount update'."
        )
    })?;

    let resolver = MountResolver::new()?;
    let mount = Mount::Git {
        url: tm.remote.clone(),
        subpath: tm.subpath.clone(),
        sync: tm.sync,
    };

    resolver
        .resolve_mount(&mount)
        .context("Thoughts mount not cloned. Run 'thoughts sync' or 'thoughts mount update' first.")
}

/// Public helper for commands that must not create dirs (e.g., work complete).
/// Runs migration and auto-archive, then enforces branch lockout.
pub fn check_branch_allowed() -> Result<()> {
    let thoughts_root = resolve_thoughts_root()?;
    // Preserve legacy migration then auto-archive
    migrate_active_layer(&thoughts_root)?;
    auto_archive_weekly_dirs(&thoughts_root)?;
    let code_root = find_repo_root(&std::env::current_dir()?)?;
    let branch = get_current_branch(&code_root)?;
    if is_main_like(&branch) {
        return Err(main_branch_lockout_error(&branch));
    }
    Ok(())
}

/// Ensure active work directory exists with subdirs and manifest.
/// Fails on main/master; never creates weekly directories.
pub fn ensure_active_work() -> Result<ActiveWork> {
    let thoughts_root = resolve_thoughts_root()?;

    // Run migrations before any branch checks
    migrate_active_layer(&thoughts_root)?;
    auto_archive_weekly_dirs(&thoughts_root)?;

    // Get branch and enforce lockout
    let code_root = find_repo_root(&std::env::current_dir()?)?;
    let branch = get_current_branch(&code_root)?;
    if is_main_like(&branch) {
        return Err(main_branch_lockout_error(&branch));
    }

    // Use branch name directly - no weekly directories
    let dir_name = branch.clone();
    let base = thoughts_root.join(&dir_name);

    // Create structure if missing
    if !base.exists() {
        fs::create_dir_all(base.join("research")).context("Failed to create research directory")?;
        fs::create_dir_all(base.join("plans")).context("Failed to create plans directory")?;
        fs::create_dir_all(base.join("artifacts"))
            .context("Failed to create artifacts directory")?;

        // Create manifest.json atomically
        let source_repo = get_remote_url(&code_root).unwrap_or_else(|_| "unknown".to_string());
        let manifest = json!({
            "source_repo": source_repo,
            "branch_or_week": dir_name,
            "started_at": chrono::Utc::now().to_rfc3339(),
        });

        let manifest_path = base.join("manifest.json");
        AtomicFile::new(&manifest_path, OverwriteBehavior::AllowOverwrite)
            .write(|f| f.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes()))
            .with_context(|| format!("Failed to write manifest at {}", manifest_path.display()))?;
    } else {
        // Ensure subdirs exist even if base exists
        for sub in ["research", "plans", "artifacts"] {
            let subdir = base.join(sub);
            if !subdir.exists() {
                fs::create_dir_all(&subdir)
                    .with_context(|| format!("Failed to ensure {} directory", sub))?;
            }
        }
        // Ensure manifest exists
        let manifest_path = base.join("manifest.json");
        if !manifest_path.exists() {
            let source_repo = get_remote_url(&code_root).unwrap_or_else(|_| "unknown".to_string());
            let manifest = json!({
                "source_repo": source_repo,
                "branch_or_week": dir_name,
                "started_at": chrono::Utc::now().to_rfc3339(),
            });
            AtomicFile::new(&manifest_path, OverwriteBehavior::AllowOverwrite)
                .write(|f| f.write_all(serde_json::to_string_pretty(&manifest)?.as_bytes()))
                .with_context(|| {
                    format!("Failed to write manifest at {}", manifest_path.display())
                })?;
        }
    }

    Ok(ActiveWork {
        dir_name: dir_name.clone(),
        base: base.clone(),
        research: base.join("research"),
        plans: base.join("plans"),
        artifacts: base.join("artifacts"),
    })
}

#[cfg(test)]
mod branch_lock_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn is_main_like_detection() {
        assert!(is_main_like("main"));
        assert!(is_main_like("master"));
        assert!(!is_main_like("feature/login"));
        assert!(!is_main_like("main-feature"));
        assert!(!is_main_like("my-master"));
    }

    #[test]
    fn weekly_name_detection() {
        // Valid new format: YYYY-WWW
        assert!(is_weekly_dir_name("2025-W01"));
        assert!(is_weekly_dir_name("2024-W53"));
        assert!(is_weekly_dir_name("2020-W10"));

        // Valid legacy format: YYYY_week_WW
        assert!(is_weekly_dir_name("2024_week_52"));
        assert!(is_weekly_dir_name("2025_week_01"));

        // Invalid: branch names
        assert!(!is_weekly_dir_name("feat/login-page"));
        assert!(!is_weekly_dir_name("main"));
        assert!(!is_weekly_dir_name("master"));
        assert!(!is_weekly_dir_name("feature-2025-W01"));

        // Invalid: out of range weeks
        assert!(!is_weekly_dir_name("2025-W00"));
        assert!(!is_weekly_dir_name("2025-W54"));
        assert!(!is_weekly_dir_name("2025_week_00"));
        assert!(!is_weekly_dir_name("2025_week_54"));

        // Invalid: malformed
        assert!(!is_weekly_dir_name("2025-W1")); // single digit week
        assert!(!is_weekly_dir_name("202-W01")); // 3 digit year
        assert!(!is_weekly_dir_name("2025_week_1")); // single digit week
    }

    #[test]
    fn auto_archive_moves_weekly_dirs() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create weekly dirs to archive
        fs::create_dir_all(root.join("2025-W01")).unwrap();
        fs::create_dir_all(root.join("2024_week_52")).unwrap();
        // Create non-weekly dir that should NOT be archived
        fs::create_dir_all(root.join("feature-branch")).unwrap();

        auto_archive_weekly_dirs(root).unwrap();

        // Weekly dirs should be moved to completed/
        assert!(!root.join("2025-W01").exists());
        assert!(!root.join("2024_week_52").exists());
        assert!(root.join("completed/2025-W01").exists());
        assert!(root.join("completed/2024_week_52").exists());

        // Non-weekly dir should remain
        assert!(root.join("feature-branch").exists());
    }

    #[test]
    fn auto_archive_handles_collision() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create completed dir with existing entry
        fs::create_dir_all(root.join("completed/2025-W01")).unwrap();
        // Create weekly dir to archive (will collide)
        fs::create_dir_all(root.join("2025-W01")).unwrap();

        auto_archive_weekly_dirs(root).unwrap();

        // Should be archived with -migrated suffix
        assert!(!root.join("2025-W01").exists());
        assert!(root.join("completed/2025-W01").exists());
        assert!(root.join("completed/2025-W01-migrated").exists());
    }

    #[test]
    fn auto_archive_idempotent() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // No weekly dirs to archive
        fs::create_dir_all(root.join("feature-branch")).unwrap();
        fs::create_dir_all(root.join("completed")).unwrap();

        // Should not fail and should not move anything
        auto_archive_weekly_dirs(root).unwrap();
        auto_archive_weekly_dirs(root).unwrap();

        assert!(root.join("feature-branch").exists());
    }

    #[test]
    fn lockout_error_message_format() {
        let err = main_branch_lockout_error("main");
        let msg = err.to_string();
        // Verify standardized message components
        assert!(msg.contains("Branch protection"));
        assert!(msg.contains("'main'"));
        assert!(msg.contains("git checkout -b"));
        assert!(msg.contains("work list"));
    }
}
