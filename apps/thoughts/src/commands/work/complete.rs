use crate::config::{Mount, RepoConfigManager};
use crate::git::utils::{find_repo_root, get_control_repo_root, get_current_branch};
use crate::mount::MountResolver;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::fs;
use thoughts_tool::workspace::check_branch_allowed;

pub async fn execute() -> Result<()> {
    // Enforce lockout and run migration/auto-archive without creating directories
    check_branch_allowed()?;

    let code_root = find_repo_root(&std::env::current_dir()?)?;
    let branch = get_current_branch(&code_root)?;

    let mgr = RepoConfigManager::new(get_control_repo_root(&std::env::current_dir()?)?);
    let ds = mgr.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    let tm = ds
        .thoughts_mount
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No thoughts_mount configured"))?;

    // Resolve the thoughts mount
    let resolver = MountResolver::new()?;
    let mount = Mount::Git {
        url: tm.remote.clone(),
        subpath: tm.subpath.clone(),
        sync: tm.sync,
    };

    let thoughts_root = resolver
        .resolve_mount(&mount)
        .context("Thoughts mount not cloned")?;

    // Use branch name directly - no weekly directories
    let dir_name = branch.clone();
    let active_dir = thoughts_root.join(&dir_name);

    if !active_dir.exists() {
        anyhow::bail!(
            "No active work directory found for current branch: {}\nExpected: {}",
            dir_name,
            active_dir.display()
        );
    }

    // Read manifest
    let manifest_path = active_dir.join("manifest.json");
    if !manifest_path.exists() {
        anyhow::bail!("No manifest.json found in work directory");
    }

    let manifest_contents = fs::read_to_string(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_contents)?;

    let started_at_str = manifest
        .get("started_at")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid manifest: missing started_at"))?;

    let started_at = DateTime::parse_from_rfc3339(started_at_str)?.date_naive();
    let ended_at = Utc::now().date_naive();

    // Format dates
    let start_date = started_at.format("%Y-%m-%d").to_string();
    let end_date = ended_at.format("%Y-%m-%d").to_string();

    // Build completed directory name with date range
    let completed_name = format!("{}_to_{}_{}", start_date, end_date, dir_name);
    let completed_dir = thoughts_root.join("completed").join(&completed_name);

    // Ensure completed directory exists
    fs::create_dir_all(completed_dir.parent().unwrap())?;

    // Move directory
    fs::rename(&active_dir, &completed_dir)?;

    println!(
        "{} Completed work: {}",
        "✓".green(),
        completed_dir.display()
    );
    println!("  Duration: {} to {}", start_date, end_date);

    Ok(())
}
