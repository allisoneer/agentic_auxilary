use crate::config::{Mount, SyncStrategy, MountMerger, RepoMappingManager};
use crate::git::utils::{find_repo_root, get_remote_url};
use crate::git::GitSync;
use crate::mount::MountResolver;
use anyhow::{Result, Context};
use colored::*;
use std::env;

pub async fn execute(mount_name: Option<String>, all: bool) -> Result<()> {
    // Validate arguments
    if mount_name.is_some() && all {
        anyhow::bail!("Cannot specify both a mount name and --all");
    }
    
    let repo_root = find_repo_root(&env::current_dir()?)?;
    let merger = MountMerger::new(repo_root.clone());
    let resolver = MountResolver::new()?;
    
    // Get all mounts from merged config
    let all_mounts = merger.get_all_mounts().await?;
    
    // Determine mounts to sync
    let mounts_to_sync = if let Some(name) = mount_name {
        // Specific mount requested
        if all_mounts.contains_key(&name) {
            vec![name]
        } else {
            anyhow::bail!("Mount '{}' not found", name);
        }
    } else if all {
        // All auto-sync mounts
        all_mounts.iter()
            .filter(|(_, (mount, _))| mount.sync_strategy() == SyncStrategy::Auto)
            .map(|(name, _)| name.clone())
            .collect()
    } else {
        // Repository-aware sync (default) - sync all auto mounts for current repo
        // Note: get_all_mounts() already returns only mounts configured for this repository
        let repo_url = get_remote_url(&repo_root)?;
        println!("{} repository: {}", "Detected".cyan(), repo_url);
        
        all_mounts.iter()
            .filter(|(_, (mount, _))| mount.sync_strategy() == SyncStrategy::Auto)
            .map(|(name, _)| name.clone())
            .collect()
    };
    
    if mounts_to_sync.is_empty() {
        println!("{}: No mounts to sync", "Info".yellow());
        if !all {
            println!("Try '{}' to sync all auto-sync mounts", "thoughts sync --all".cyan());
        }
        return Ok(());
    }
    
    println!("{} {} mount(s)...", "Syncing".green(), mounts_to_sync.len());
    
    // Sync each mount
    for mount_name in &mounts_to_sync {
        if let Some((mount, _source)) = all_mounts.get(mount_name) {
            match sync_mount(mount_name, mount, &resolver).await {
                Ok(_) => println!("  {} {}", "✓".green(), mount_name),
                Err(e) => eprintln!("  {} {}: {}", "✗".red(), mount_name, e),
            }
        }
    }
    
    Ok(())
}

async fn sync_mount(name: &str, mount: &Mount, resolver: &MountResolver) -> Result<()> {
    let mount_path = resolver.resolve_mount(mount)
        .context("Mount not cloned")?;
    
    if !mount_path.exists() {
        anyhow::bail!("Mount path does not exist: {}", mount_path.display());
    }
    
    // Only sync git mounts
    match mount {
        Mount::Git { url, subpath, .. } => {
            // Determine repository root and sync subpath
            let (repo_root, sync_subpath) = if let Some(sub) = subpath {
                // Mount path includes subpath, we need the repo root
                // mount_path = /path/to/repo/subdir
                // We need: repo_root = /path/to/repo, sync_subpath = subdir
                let repo_root = mount_path
                    .ancestors()
                    .find(|p| p.join(".git").exists())
                    .ok_or_else(|| anyhow::anyhow!("Could not find repository root"))?
                    .to_path_buf();
                (repo_root, Some(sub.clone()))
            } else {
                // Mount path is the repo root
                (mount_path.clone(), None)
            };
            
            // Perform git sync
            let git_sync = GitSync::new(&repo_root, sync_subpath)?;
            git_sync.sync(name).await?;
            
            // Update last sync time
            let mut repo_mapping = RepoMappingManager::new()?;
            repo_mapping.update_sync_time(url)?;
        }
        Mount::Directory { .. } => {
            // Directory mounts don't sync
            println!("  {} {} (directory mount)", "Skipping".dimmed(), name);
        }
    }
    
    Ok(())
}