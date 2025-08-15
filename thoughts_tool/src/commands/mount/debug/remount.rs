use anyhow::{Result, bail};
use colored::Colorize;
use tracing::{error, info, warn};

use crate::config::MountMerger;
use crate::git::utils::find_repo_root;
use crate::mount::{MountResolver, get_mount_manager};
use crate::platform::detect_platform;

pub async fn execute(mount_name: String) -> Result<()> {
    // Get repository root
    let repo_root = find_repo_root(&std::env::current_dir()?)?;

    // Get merged configuration
    let merger = MountMerger::new(repo_root.clone());
    let merged_config = merger.get_all_mounts().await?;

    // Find mount in merged config
    let (mount, _source) = merged_config
        .get(&mount_name)
        .ok_or_else(|| anyhow::anyhow!("Mount '{}' not found in configuration", mount_name))?;

    // Resolve mount sources
    let resolver = MountResolver::new()?;
    let source_path = resolver.resolve_mount(mount)?;
    let sources = vec![source_path];

    // Get mount target
    let target = repo_root.join("thoughts").join(&mount_name);

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
