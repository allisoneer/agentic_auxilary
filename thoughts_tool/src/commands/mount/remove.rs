use crate::config::RepoConfigManager;
use crate::git::utils::get_control_repo_root;
use crate::mount::MountSpace;
use anyhow::{Result, bail};
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

    // Check if this is a context mount
    if let Some(mut config) = repo_manager.load()? {
        let initial_len = config.requires.len();
        config.requires.retain(|r| r.mount_path != mount_name);

        if config.requires.len() < initial_len {
            repo_manager.save(&config)?;
            println!("✓ Removed mount '{mount_name}'");

            // Automatically update active mounts (unmount if needed)
            crate::mount::auto_mount::update_active_mounts().await?;
        } else {
            bail!(
                "Mount '{}' not found in repository configuration",
                mount_name
            );
        }
    } else {
        bail!("No repository configuration found");
    }

    Ok(())
}
