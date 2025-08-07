use crate::config::{RepoConfigManager, PersonalConfigManager, MountMerger, MountSource};
use crate::git::utils::{find_repo_root, get_remote_url};
use anyhow::{Result, Context, bail};
use colored::Colorize;
use std::env;

pub async fn execute(mount_name: String) -> Result<()> {
    println!("{} mount '{}'...", "Removing".yellow(), mount_name);
    
    let repo_root = find_repo_root(&env::current_dir()?)?;
    let repo_url = get_remote_url(&repo_root)?;
    let merger = MountMerger::new(repo_root.clone());
    
    // Check where the mount exists
    let all_mounts = merger.get_all_mounts().await?;
    
    if let Some((_mount, source)) = all_mounts.get(&mount_name) {
        match source {
            MountSource::Repository => {
                // Remove from repository config
                let repo_manager = RepoConfigManager::new(repo_root.clone());
                if let Some(mut config) = repo_manager.load()? {
                    config.requires.retain(|r| r.mount_path != mount_name);
                    repo_manager.save(&config)?;
                    println!("✓ Removed repository mount '{}'", mount_name);
                }
            }
            MountSource::Personal => {
                // Remove from personal config for this repository
                if PersonalConfigManager::remove_repository_mount(&repo_url, &mount_name)? {
                    println!("✓ Removed personal mount '{}'", mount_name);
                } else {
                    bail!("Failed to remove personal mount");
                }
            }
            MountSource::Pattern => {
                bail!("Cannot remove pattern-based mount '{}'. Modify the pattern in personal config.", mount_name);
            }
        }
        
        // Automatically update active mounts (unmount if needed)
        crate::mount::auto_mount::update_active_mounts().await?;
    } else {
        bail!("Mount '{}' not found", mount_name);
    }
    
    Ok(())
}
