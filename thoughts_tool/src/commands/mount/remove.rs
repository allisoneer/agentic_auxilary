use crate::config::RepoConfigManager;
use crate::git::utils::find_repo_root;
use anyhow::{Result, bail};
use colored::Colorize;
use std::env;

pub async fn execute(mount_name: String) -> Result<()> {
    println!("{} mount '{}'...", "Removing".yellow(), mount_name);

    let repo_root = find_repo_root(&env::current_dir()?)?;
    let repo_manager = RepoConfigManager::new(repo_root.clone());

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
