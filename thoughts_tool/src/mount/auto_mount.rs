use crate::config::RepoMappingManager;
use crate::config::{Mount, MountMerger, MountSource, RepoConfigManager};
use crate::git::clone::{CloneOptions, clone_repository};
use crate::git::utils::find_repo_root;
use crate::mount::MountResolver;
use crate::mount::{MountManager, MountOptions, get_mount_manager};
use crate::platform::detect_platform;
use crate::utils::paths::ensure_dir;
use anyhow::{Context, Result};
use colored::*;
use std::collections::HashMap;
use std::path::PathBuf;

pub async fn update_active_mounts() -> Result<()> {
    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let merger = MountMerger::new(repo_root.clone());
    let platform_info = detect_platform()?;
    let mount_manager = get_mount_manager(&platform_info)?;

    println!("{} filesystem mounts...", "Synchronizing".cyan());

    // Get desired mounts from config
    let desired_mounts = merger.get_all_mounts().await?;

    // Get currently mounted - need to check both context and personal directories
    let active_mounts = mount_manager.list_mounts().await?;
    let mut active_map: HashMap<String, PathBuf> = HashMap::new();

    // Look for mounts in .thoughts-data subdirectories
    for mount_info in &active_mounts {
        // Check if this mount is in our .thoughts-data directory
        if mount_info
            .target
            .starts_with(repo_root.join(".thoughts-data"))
        {
            if let Some(name) = mount_info.target.file_name().and_then(|n| n.to_str()) {
                active_map.insert(name.to_string(), mount_info.target.clone());
            }
        }
    }

    // Phase 1: Unmount removed mounts
    for (active_name, active_path) in &active_map {
        if !desired_mounts.contains_key(active_name) {
            println!("  {} removed mount: {}", "Unmounting".yellow(), active_name);
            mount_manager
                .unmount(&active_path, false)
                .await
                .context(format!("Failed to unmount {}", active_name))?;
        }
    }

    // Phase 2: Mount new/missing mounts
    // Load repository config to get mount directories
    let repo_manager = RepoConfigManager::new(repo_root.clone());
    let repo_config = repo_manager.load()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init' first.")
    })?;

    // Mount base is in the repository's .thoughts-data directory
    let thoughts_data_base = repo_root.join(".thoughts-data");
    ensure_dir(&thoughts_data_base)?;

    // Ensure mount directories exist
    let context_dir = thoughts_data_base.join(&repo_config.mount_dirs.repository);
    let personal_dir = thoughts_data_base.join(&repo_config.mount_dirs.personal);
    ensure_dir(&context_dir)?;
    ensure_dir(&personal_dir)?;

    for (name, (mount, source)) in &desired_mounts {
        if !active_map.contains_key(name) {
            println!(
                "  {} {}: {} ({})",
                "Mounting".green(),
                name,
                mount_description(mount),
                format!("{:?}", source).to_lowercase()
            );

            // Resolve mount path
            let resolver = MountResolver::new()?;
            let mount_path = match resolver.resolve_mount(mount) {
                Ok(path) => path,
                Err(_) => {
                    // Need to clone first
                    if let Mount::Git { url, .. } = mount {
                        if resolver.needs_clone(mount)? {
                            println!("    {} repository...", "Cloning".yellow());
                            clone_and_map(url, name).await?
                        } else {
                            eprintln!("    {} Failed to resolve mount path", "Error:".red());
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
            };

            // Verify mount path exists
            if !mount_path.exists() {
                eprintln!(
                    "    {} Mount path does not exist: {}",
                    "Error:".red(),
                    mount_path.display()
                );
                continue;
            }

            // Determine target directory based on mount source
            let target = match source {
                MountSource::Repository => context_dir.join(name),
                MountSource::Personal => personal_dir.join(name),
                MountSource::Pattern => personal_dir.join(name), // Patterns are personal
            };

            // Mount it
            let options = MountOptions::default();
            match mount_manager.mount(&[mount_path], &target, &options).await {
                Ok(_) => println!("    {} Successfully mounted", "✓".green()),
                Err(e) => eprintln!("    {} Failed to mount: {}", "✗".red(), e),
            }
        }
    }

    println!("{} Mount synchronization complete", "✓".green());
    Ok(())
}

async fn clone_and_map(url: &str, _name: &str) -> Result<PathBuf> {
    let mut repo_mapping = RepoMappingManager::new()?;
    let default_path = RepoMappingManager::get_default_clone_path(url)?;

    // Clone to default location
    let clone_opts = CloneOptions {
        url: url.to_string(),
        target_path: default_path.clone(),
        shallow: false,
        branch: None,
    };
    clone_repository(&clone_opts)?;

    // Add mapping
    repo_mapping.add_mapping(url.to_string(), default_path.clone(), true)?;

    Ok(default_path)
}

fn mount_description(mount: &Mount) -> String {
    match mount {
        Mount::Git { url, subpath, .. } => {
            if let Some(sub) = subpath {
                format!("{}:{}", url, sub)
            } else {
                url.clone()
            }
        }
        Mount::Directory { path, .. } => path.display().to_string(),
    }
}
