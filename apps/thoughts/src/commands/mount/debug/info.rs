use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

use crate::mount::get_mount_manager;
use crate::platform::detect_platform;

pub async fn execute(target: String) -> Result<()> {
    let platform_info = detect_platform()?;
    let mount_manager = get_mount_manager(&platform_info)?;

    let target_path = PathBuf::from(&target);

    if let Some(mount_info) = mount_manager.get_mount_info(&target_path).await? {
        println!("Mount Information for {}:", target);
        println!("  Status: {:?}", mount_info.status);
        println!("  Sources:");
        for source in &mount_info.sources {
            println!("    - {}", source.display());
        }
        println!("  Filesystem: {}", mount_info.fs_type);
        println!("  Options: {}", mount_info.options.join(", "));
        if let Some(pid) = mount_info.pid {
            println!("  Process ID: {}", pid);
        }
        if let Some(mounted_at) = mount_info.mounted_at {
            println!("  Mounted at: {:?}", mounted_at);
        }

        // Print platform-specific metadata
        match &mount_info.metadata {
            crate::mount::MountMetadata::Linux {
                mount_id,
                parent_id,
                major_minor,
            } => {
                if let Some(id) = mount_id {
                    println!("  Mount ID: {}", id);
                }
                if let Some(pid) = parent_id {
                    println!("  Parent ID: {}", pid);
                }
                if let Some(mm) = major_minor {
                    println!("  Major:Minor: {}", mm);
                }
            }
            crate::mount::MountMetadata::MacOS {
                volume_name,
                volume_uuid,
                disk_identifier,
            } => {
                if let Some(name) = volume_name {
                    println!("  Volume Name: {}", name);
                }
                if let Some(uuid) = volume_uuid {
                    println!("  Volume UUID: {}", uuid);
                }
                if let Some(disk) = disk_identifier {
                    println!("  Disk Identifier: {}", disk);
                }
            }
            crate::mount::MountMetadata::Unknown => {
                println!("  Metadata: Unknown filesystem type");
            }
        }
    } else {
        println!("No mount found at {}", target);
        info!("Mount not found: {}", target);
    }

    Ok(())
}
