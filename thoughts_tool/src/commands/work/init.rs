use super::utils::current_iso_week_dir;
use crate::config::{Mount, RepoConfigManager};
use crate::git::utils::{
    find_repo_root, get_control_repo_root, get_current_branch, get_remote_url,
};
use crate::mount::MountResolver;
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let code_root = find_repo_root(&std::env::current_dir()?)?;
    let branch = get_current_branch(&code_root)?;

    let mgr = RepoConfigManager::new(get_control_repo_root(&std::env::current_dir()?)?);
    let ds = mgr.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    let tm = ds
        .thoughts_mount
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No thoughts_mount configured in repository configuration.\n\nPlease add a thoughts_mount to your .thoughts/config.json:\n  \"thoughts_mount\": {{\n    \"remote\": \"<git-url>\",\n    \"sync\": \"auto\"\n  }}"))?;

    // Resolve the thoughts mount to its local path
    let resolver = MountResolver::new()?;
    let mount = Mount::Git {
        url: tm.remote.clone(),
        subpath: tm.subpath.clone(),
        sync: tm.sync,
    };

    let thoughts_root = resolver.resolve_mount(&mount).context(
        "Thoughts mount not cloned. Run 'thoughts sync' or 'thoughts mount update' first.",
    )?;

    // Determine directory name based on branch
    let dir_name = if branch == "main" || branch == "master" {
        current_iso_week_dir()
    } else {
        branch.clone()
    };

    // Create work directory structure
    let base = thoughts_root.join("active").join(&dir_name);

    if base.exists() {
        println!(
            "{}: Work directory already exists: {}",
            "Note".yellow(),
            base.display()
        );
        return Ok(());
    }

    std::fs::create_dir_all(base.join("research"))
        .context("Failed to create research directory")?;
    std::fs::create_dir_all(base.join("plans")).context("Failed to create plans directory")?;
    std::fs::create_dir_all(base.join("artifacts"))
        .context("Failed to create artifacts directory")?;

    // Create manifest
    let source_repo = get_remote_url(&code_root).unwrap_or_else(|_| "unknown".to_string());
    let manifest = serde_json::json!({
        "source_repo": source_repo,
        "branch_or_week": dir_name,
        "started_at": chrono::Utc::now().to_rfc3339(),
    });

    std::fs::write(
        base.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .context("Failed to write manifest")?;

    println!("{} Initialized work at: {}", "✓".green(), base.display());
    println!("  Branch/Week: {}", dir_name);
    println!("  Structure:");
    println!("    - research/   (research notes and exploration)");
    println!("    - plans/      (technical plans and designs)");
    println!("    - artifacts/  (generated outputs and results)");

    Ok(())
}
