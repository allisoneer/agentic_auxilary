use anyhow::{Context, Result, bail};
use colored::Colorize;
use tracing::{error, info, warn};

use crate::config::RepoConfigManager;
use crate::git::utils::find_repo_root;
use crate::mount::{MountResolver, MountSpace, get_mount_manager};
use crate::platform::detect_platform;

pub async fn execute(mount_name: String) -> Result<()> {
    // Get repository root
    let repo_root = find_repo_root(&std::env::current_dir()?)?;

    // Get configuration
    let repo_manager = RepoConfigManager::new(repo_root.clone());
    let desired = repo_manager
        .load_desired_state()?
        .ok_or_else(|| anyhow::anyhow!("No repository configuration found"))?;

    // Parse mount name to MountSpace
    let mount_space = MountSpace::parse(&mount_name)
        .with_context(|| format!("Invalid mount name: {}", mount_name))?;

    // Find mount using MountSpace
    let mount = desired
        .find_mount(&mount_space)
        .ok_or_else(|| anyhow::anyhow!("Mount '{}' not found in configuration", mount_name))?;

    // Resolve mount sources
    let resolver = MountResolver::new()?;
    let source_path = resolver.resolve_mount(&mount)?;
    let sources = vec![source_path];

    // Get mount target using MountSpace
    let target = desired.get_mount_target(&mount_space, &repo_root);

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
