use anyhow::Result;
use tracing::info;

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
