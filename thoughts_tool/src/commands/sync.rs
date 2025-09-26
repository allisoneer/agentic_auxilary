use crate::config::{Mount, RepoConfigManager, RepoMappingManager, SyncStrategy};
use crate::git::GitSync;
use crate::git::utils::{find_repo_root, get_remote_url};
use crate::mount::MountResolver;
use anyhow::{Context, Result};
use colored::*;
use std::env;

pub async fn execute(mount_name: Option<String>, all: bool) -> Result<()> {
    // Validate arguments
    if mount_name.is_some() && all {
        anyhow::bail!("Cannot specify both a mount name and --all");
    }

    let repo_root = find_repo_root(&env::current_dir()?)?;
    let repo_manager = RepoConfigManager::new(repo_root.clone());
    let resolver = MountResolver::new()?;

    let desired = repo_manager
        .load_desired_state()?
        .ok_or_else(|| anyhow::anyhow!("No repository configuration found"))?;

    // Build mount list from DesiredState
    let mut sync_list = vec![];

    // Add thoughts mount if configured and auto-sync
    if let Some(tm) = &desired.thoughts_mount
        && tm.sync == SyncStrategy::Auto
    {
        sync_list.push((
            "thoughts".to_string(),
            Mount::Git {
                url: tm.remote.clone(),
                sync: tm.sync,
                subpath: tm.subpath.clone(),
            },
        ));
    }

    // Add context mounts that are auto-sync
    for cm in &desired.context_mounts {
        if cm.sync == SyncStrategy::Auto {
            sync_list.push((
                cm.mount_path.clone(),
                Mount::Git {
                    url: cm.remote.clone(),
                    sync: cm.sync,
                    subpath: cm.subpath.clone(),
                },
            ));
        }
    }

    // Determine mounts to sync
    let mounts_to_sync = if let Some(name) = mount_name {
        // Specific mount requested
        if sync_list.iter().any(|(n, _)| n == &name) {
            vec![name]
        } else {
            anyhow::bail!("Mount '{}' not found or not configured for sync", name);
        }
    } else if all {
        // All auto-sync mounts
        sync_list.iter().map(|(n, _)| n.clone()).collect()
    } else {
        // Repository-aware sync (default) - sync all auto mounts for current repo
        let repo_url = get_remote_url(&repo_root)?;
        println!("{} repository: {}", "Detected".cyan(), repo_url);

        sync_list.iter().map(|(n, _)| n.clone()).collect()
    };

    if mounts_to_sync.is_empty() {
        println!("{}: No mounts to sync", "Info".yellow());
        if !all {
            println!(
                "Try '{}' to sync all auto-sync mounts",
                "thoughts sync --all".cyan()
            );
        }
        return Ok(());
    }

    println!("{} {} mount(s)...", "Syncing".green(), mounts_to_sync.len());

    // Sync each mount
    for mount_name in &mounts_to_sync {
        if let Some((_, mount)) = sync_list.iter().find(|(n, _)| n == mount_name) {
            match sync_mount(mount_name, mount, &resolver).await {
                Ok(_) => println!("  {} {}", "✓".green(), mount_name),
                Err(e) => eprintln!("  {} {}: {}", "✗".red(), mount_name, e),
            }
        }
    }

    Ok(())
}

async fn sync_mount(name: &str, mount: &Mount, resolver: &MountResolver) -> Result<()> {
    let mount_path = resolver.resolve_mount(mount).context("Mount not cloned")?;

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
