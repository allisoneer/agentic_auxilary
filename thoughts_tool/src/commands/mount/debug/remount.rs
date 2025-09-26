use anyhow::{Result, bail};
use colored::Colorize;
use tracing::{error, info, warn};

use crate::config::{Mount, RepoConfigManager};
use crate::git::utils::find_repo_root;
use crate::mount::{MountResolver, get_mount_manager};
use crate::platform::detect_platform;

pub async fn execute(mount_name: String) -> Result<()> {
    // Get repository root
    let repo_root = find_repo_root(&std::env::current_dir()?)?;

    // Get configuration
    let repo_manager = RepoConfigManager::new(repo_root.clone());
    let desired = repo_manager
        .load_desired_state()?
        .ok_or_else(|| anyhow::anyhow!("No repository configuration found"))?;

    // Find mount in configuration
    let mount = if mount_name == "thoughts" && desired.thoughts_mount.is_some() {
        let tm = desired.thoughts_mount.as_ref().unwrap();
        Mount::Git {
            url: tm.remote.clone(),
            sync: tm.sync,
            subpath: tm.subpath.clone(),
        }
    } else if let Some(cm) = desired
        .context_mounts
        .iter()
        .find(|m| m.mount_path == mount_name)
    {
        Mount::Git {
            url: cm.remote.clone(),
            sync: cm.sync,
            subpath: cm.subpath.clone(),
        }
    } else {
        anyhow::bail!("Mount '{}' not found in configuration", mount_name);
    };

    // Resolve mount sources
    let resolver = MountResolver::new()?;
    let source_path = resolver.resolve_mount(&mount)?;
    let sources = vec![source_path];

    // Get mount target based on type
    let target = if mount_name == "thoughts" {
        repo_root
            .join(".thoughts-data")
            .join(&desired.mount_dirs.thoughts)
    } else {
        repo_root
            .join(".thoughts-data")
            .join(&desired.mount_dirs.context)
            .join(&mount_name)
    };

    // Get platform and mount manager
    let platform_info = detect_platform()?;
    let mount_manager = get_mount_manager(&platform_info)?;

    // Build mount options
    let options = crate::mount::MountOptions::default();

    println!("Remounting '{}'...", mount_name.yellow());

    // First unmount if currently mounted
    if mount_manager.is_mounted(&target).await? {
        println!("  Unmounting existing mount...");
        match mount_manager.unmount(&target, false).await {
            Ok(_) => println!("  {} Unmounted successfully", "✓".green()),
            Err(e) => {
                warn!("Failed to unmount cleanly: {}", e);
                println!("  {} Unmount failed (may be busy)", "⚠".yellow());
                println!("  Attempting force unmount...");

                match mount_manager.unmount(&target, true).await {
                    Ok(_) => println!("  {} Force unmount successful", "✓".green()),
                    Err(e) => {
                        error!("Force unmount failed: {}", e);
                        println!("  {} Force unmount failed", "✗".red());
                        bail!("Cannot remount: unmount failed");
                    }
                }
            }
        }
    } else {
        println!("  Mount not currently active");
    }

    // Now mount with fresh state
    println!("  Mounting with current configuration...");
    match mount_manager.mount(&sources, &target, &options).await {
        Ok(_) => {
            println!("  {} Mount successful", "✓".green());
            println!();
            println!(
                "{} '{}' has been remounted successfully",
                "✓".green(),
                mount_name
            );
            info!("Successfully remounted {}", mount_name);
        }
        Err(e) => {
            error!("Mount failed: {}", e);
            println!("  {} Mount failed: {}", "✗".red(), e);
            bail!("Remount failed");
        }
    }

    Ok(())
}
