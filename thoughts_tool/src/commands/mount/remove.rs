use crate::config::RepoConfigManager;
use crate::git::utils::get_control_repo_root;
use crate::mount::MountSpace;
use anyhow::Result;
use colored::Colorize;
use std::env;

pub async fn execute(mount_name: String) -> Result<()> {
    let repo_root = get_control_repo_root(&env::current_dir()?)?;
    let repo_manager = RepoConfigManager::new(repo_root.clone());

    // Parse to MountSpace for validation
    let mount_space = MountSpace::parse(&mount_name)?;

    // Only context mounts can be removed
    match mount_space {
        MountSpace::Context(_) => {
            // Proceed with removal
        }
        MountSpace::Thoughts => {
            anyhow::bail!("Cannot remove the thoughts mount");
        }
        MountSpace::Reference { .. } => {
            anyhow::bail!("Use 'thoughts references remove' to remove references");
        }
    }

    println!("{} mount '{}'...", "Removing".yellow(), mount_name);

    let mut cfg = repo_manager.load_v2_or_bail()?;
    let before = cfg.context_mounts.len();
    cfg.context_mounts.retain(|m| m.mount_path != mount_name);
    if cfg.context_mounts.len() == before {
        println!("No mount named '{}' found", mount_name);
        return Ok(());
    }
    let warnings = repo_manager.save_v2_validated(&cfg)?;
    for w in warnings {
        eprintln!("Warning: {}", w);
    }
    println!("✓ Removed mount '{}'", mount_name);

    // Automatically update active mounts (unmount if needed)
    crate::mount::auto_mount::update_active_mounts().await?;

    Ok(())
}
