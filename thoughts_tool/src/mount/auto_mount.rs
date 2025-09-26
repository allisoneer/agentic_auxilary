use crate::config::RepoMappingManager;
use crate::config::{Mount, RepoConfigManager, SyncStrategy, extract_org_repo_from_url};
use crate::git::clone::{CloneOptions, clone_repository};
use crate::git::utils::find_repo_root;
use crate::mount::MountResolver;
use crate::mount::{MountOptions, get_mount_manager};
use crate::platform::detect_platform;
use crate::utils::paths::ensure_dir;
use anyhow::Result;
use colored::*;
use std::collections::HashMap;
use std::path::PathBuf;

pub async fn update_active_mounts() -> Result<()> {
    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let platform_info = detect_platform()?;
    let mount_manager = get_mount_manager(&platform_info)?;
    let repo_manager = RepoConfigManager::new(repo_root.clone());
    let desired = repo_manager.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    let base = repo_root.join(".thoughts-data");
    ensure_dir(&base)?;

    // Symlink targets (actual mount dirs)
    let thoughts_dir = base.join(&desired.mount_dirs.thoughts);
    let context_dir = base.join(&desired.mount_dirs.context);
    let references_dir = base.join(&desired.mount_dirs.references);
    ensure_dir(&thoughts_dir)?;
    ensure_dir(&context_dir)?;
    ensure_dir(&references_dir)?;

    println!("{} filesystem mounts...", "Synchronizing".cyan());

    // Build desired target map: key = relative path (e.g., "context/api-docs" or "references/org/repo" or "thoughts")
    let mut desired_targets: Vec<(String, Mount, bool)> = vec![]; // (target_key, mount, read_only)

    if let Some(tm) = &desired.thoughts_mount {
        let m = Mount::Git {
            url: tm.remote.clone(),
            subpath: tm.subpath.clone(),
            sync: tm.sync,
        };
        desired_targets.push((desired.mount_dirs.thoughts.clone(), m, false));
    }

    for cm in &desired.context_mounts {
        let m = Mount::Git {
            url: cm.remote.clone(),
            subpath: cm.subpath.clone(),
            sync: cm.sync,
        };
        let key = format!("{}/{}", desired.mount_dirs.context, cm.mount_path);
        desired_targets.push((key, m, false));
    }

    for url in &desired.references {
        let (org, repo) = extract_org_repo_from_url(url)?;
        let key = format!("{}/{}/{}", desired.mount_dirs.references, org, repo);
        let m = Mount::Git {
            url: url.clone(),
            subpath: None,
            sync: SyncStrategy::None,
        };
        desired_targets.push((key, m, true));
    }

    // Query active mounts and key them by relative path under .thoughts-data
    let active = mount_manager.list_mounts().await?;
    let mut active_map = HashMap::<String, PathBuf>::new();
    for mi in active {
        if mi.target.starts_with(&base)
            && let Ok(rel) = mi.target.strip_prefix(&base)
        {
            let key = rel.to_string_lossy().to_string();
            active_map.insert(key, mi.target.clone());
        }
    }

    // Unmount no-longer-desired
    for (active_key, target_path) in &active_map {
        if !desired_targets.iter().any(|(k, _, _)| k == active_key) {
            println!("  {} removed mount: {}", "Unmounting".yellow(), active_key);
            mount_manager.unmount(target_path, false).await?;
        }
    }

    // Mount missing
    let resolver = MountResolver::new()?;
    for (key, m, read_only) in desired_targets {
        if !active_map.contains_key(&key) {
            let target = base.join(&key);
            ensure_dir(target.parent().unwrap())?;

            // Resolve mount source
            let src = match resolver.resolve_mount(&m) {
                Ok(p) => p,
                Err(_) => {
                    if let Mount::Git { url, .. } = &m {
                        println!("  {} repository {} ...", "Cloning".yellow(), url);
                        clone_and_map(url, &key).await?
                    } else {
                        continue;
                    }
                }
            };

            // Mount with appropriate options
            let mut options = MountOptions::default();
            if read_only {
                options.read_only = true;
            }

            println!(
                "  {} {}: {}",
                "Mounting".green(),
                key,
                if read_only { "(read-only)" } else { "" }
            );

            match mount_manager.mount(&[src], &target, &options).await {
                Ok(_) => println!("    {} Successfully mounted", "✓".green()),
                Err(e) => eprintln!("    {} Failed to mount: {}", "✗".red(), e),
            }
        }
    }

    println!("{} Mount synchronization complete", "✓".green());
    Ok(())
}

async fn clone_and_map(url: &str, _key: &str) -> Result<PathBuf> {
    let mut repo_mapping = RepoMappingManager::new()?;
    let default_path = RepoMappingManager::get_default_clone_path(url)?;

    // Clone to default location
    let clone_opts = CloneOptions {
        url: url.to_string(),
        target_path: default_path.clone(),
        branch: None,
    };
    clone_repository(&clone_opts)?;

    // Add mapping
    repo_mapping.add_mapping(url.to_string(), default_path.clone(), true)?;

    Ok(default_path)
}
