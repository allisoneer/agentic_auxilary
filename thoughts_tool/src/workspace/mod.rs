use anyhow::{Context, Result};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use chrono::Datelike;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::config::{Mount, RepoConfigManager};
use crate::git::utils::{
    find_repo_root, get_control_repo_root, get_current_branch, get_remote_url,
};
use crate::mount::MountResolver;

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

/// Compute work directory name: ISO week for main/master, branch name otherwise
fn current_work_dir_name() -> Result<String> {
    let code_root = find_repo_root(&std::env::current_dir()?)?;
    let branch = get_current_branch(&code_root)?;
    if branch == "main" || branch == "master" {
        let now = chrono::Utc::now().date_naive();
        let iso = now.iso_week();
        Ok(format!("{}-W{:02}", iso.year(), iso.week()))
    } else {
        Ok(branch)
    }
}

/// Ensure active work directory exists with subdirs and manifest
pub fn ensure_active_work() -> Result<ActiveWork> {
    let thoughts_root = resolve_thoughts_root()?;
    let dir_name = current_work_dir_name()?;
    let base = thoughts_root.join("active").join(&dir_name);

    // Create structure if missing
    if !base.exists() {
        fs::create_dir_all(&base).context("Failed to create base work directory")?;
        fs::create_dir_all(base.join("research")).context("Failed to create research directory")?;
        fs::create_dir_all(base.join("plans")).context("Failed to create plans directory")?;
        fs::create_dir_all(base.join("artifacts"))
            .context("Failed to create artifacts directory")?;

        // Create manifest.json atomically
        let code_root = find_repo_root(&std::env::current_dir()?)?;
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
            let code_root = find_repo_root(&std::env::current_dir()?)?;
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
