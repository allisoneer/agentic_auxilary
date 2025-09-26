use anyhow::Result;
use tracing::info;

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
