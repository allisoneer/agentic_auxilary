use anyhow::{Context, Result};
use tracing::info;

use crate::config::RepoConfigManager;
use crate::git::utils::get_control_repo_root;
use crate::mount::{MountResolver, MountSpace, get_mount_manager};
use crate::platform::detect_platform;

pub async fn execute(mount_name: String) -> Result<()> {
    // Get repository root
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;

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

    // Get the mount command
    let command = mount_manager.get_mount_command(&sources, &target, &options);

    println!("Mount command for '{}':", mount_name);
    println!();
    println!("{}", command);
    println!();
    println!("Sources:");
    for source in &sources {
        println!("  - {}", source.display());
    }
    println!("Target: {}", target.display());

    info!("Displayed mount command for {}", mount_name);

    Ok(())
}
