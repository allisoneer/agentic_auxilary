use crate::config::{Mount, RepoConfigManager, RepoMappingManager};
use crate::git::utils::{find_repo_root, get_control_repo_root};
use crate::mount::{MountResolver, get_mount_manager};
use crate::platform::detect_platform;
use anyhow::Result;
use colored::*;
use std::env;

pub async fn execute(_detailed: bool) -> Result<()> {
    let code_root = match find_repo_root(&env::current_dir()?) {
        Ok(root) => root,
        Err(_) => {
            println!("{}: Not in a git repository", "Error".red());
            println!(
                "Run {} from within a git repository",
                "thoughts status".cyan()
            );
            return Ok(());
        }
    };

    let control_root = get_control_repo_root(&env::current_dir()?)?;

    let repo_manager = RepoConfigManager::new(control_root.clone());
    let platform_info = detect_platform()?;
    let mount_manager = get_mount_manager(&platform_info)?;
    let resolver = MountResolver::new()?;
    let repo_mapping = RepoMappingManager::new()?;

    // Get configuration
    let desired = repo_manager.load_desired_state()?;

    println!("{}", "Thoughts Tool Status".bold().cyan());
    println!("{}", "===================".cyan());
    println!();

    // Repository info
    println!("{}", "Repository:".bold());
    println!("  Path: {}", code_root.display());
    if let Ok(url) = crate::git::utils::get_remote_url(&code_root) {
        println!("  Remote: {url}");
    }
    println!();

    // Configuration status
    if let Some(ds) = &desired
        && ds.was_v1
    {
        println!("  {}: Using v1 configuration (legacy)", "Note".yellow());
        println!("  Personal mounts are deprecated and ignored");
        println!();
    }

    // Mount status
    println!("{}", "Mounts:".bold());

    if desired.is_none() {
        println!("  No configuration found");
        println!();
        println!("  Initialize with:");
        println!("    {}", "thoughts init".cyan());
        return Ok(());
    }

    let ds = desired.unwrap();
    let has_mounts =
        ds.thoughts_mount.is_some() || !ds.context_mounts.is_empty() || !ds.references.is_empty();

    if !has_mounts {
        println!("  No mounts configured");
        println!();
        println!("  Add mounts with:");
        println!("    {}", "thoughts mount add <path>".cyan());
    } else {
        // Show thoughts mount
        if let Some(tm) = &ds.thoughts_mount {
            println!(
                "\n  {} {}:",
                "thoughts".cyan().bold(),
                "(workspace)".dimmed()
            );
            let display_url = if let Some(sub) = &tm.subpath {
                format!("{}:{}", tm.remote, sub)
            } else {
                tm.remote.clone()
            };
            println!("    URL: {display_url}");
            println!("    Sync: {}", tm.sync);

            // Show local path and status
            let mount = Mount::Git {
                url: tm.remote.clone(),
                sync: tm.sync,
                subpath: tm.subpath.clone(),
            };
            show_mount_status(&mount, &resolver, &repo_mapping, &tm.remote).await?;

            // Check if mounted
            let target = control_root
                .join(".thoughts-data")
                .join(&ds.mount_dirs.thoughts);
            let is_mounted = mount_manager.is_mounted(&target).await?;
            if is_mounted {
                println!("    Status: {} ✓", "Mounted".green().bold());
            } else {
                println!("    Status: {} ✗", "Not mounted".red().bold());
                println!("    Tip: Run {} to mount", "thoughts mount update".cyan());
            }
        }

        // Show context mounts
        if !ds.context_mounts.is_empty() {
            println!("\n  {}:", "Context mounts".yellow().bold());
            for cm in &ds.context_mounts {
                println!("    {}:", cm.mount_path.cyan());
                let display_url = if let Some(sub) = &cm.subpath {
                    format!("{}:{}", cm.remote, sub)
                } else {
                    cm.remote.clone()
                };
                println!("      URL: {display_url}");
                println!("      Sync: {}", cm.sync);

                // Show local path and status
                let mount = Mount::Git {
                    url: cm.remote.clone(),
                    sync: cm.sync,
                    subpath: cm.subpath.clone(),
                };
                show_mount_status(&mount, &resolver, &repo_mapping, &cm.remote).await?;

                // Check if mounted
                let target = control_root
                    .join(".thoughts-data")
                    .join(&ds.mount_dirs.context)
                    .join(&cm.mount_path);
                let is_mounted = mount_manager.is_mounted(&target).await?;
                if is_mounted {
                    println!("      Status: {} ✓", "Mounted".green().bold());
                } else {
                    println!("      Status: {} ✗", "Not mounted".red().bold());
                }
            }
        }

        // Show references summary
        if !ds.references.is_empty() {
            println!("\n  {}:", "References".green().bold());
            println!("    Count: {} repositories", ds.references.len());
            println!("    Mount: Read-only under {}/", ds.mount_dirs.references);
            println!(
                "    Tip: Use {} for details",
                "thoughts references list".cyan()
            );
        }
    }

    println!();

    // Mount system health
    println!("{}", "System:".bold());
    match mount_manager.check_health().await {
        Ok(_) => println!("  Mount system: {} ✓", "Healthy".green()),
        Err(e) => {
            println!("  Mount system: {} ✗", "Issues detected".red());
            println!("  {}", e.to_string().dimmed());
        }
    }

    // Platform info
    let platform_str = match &platform_info.platform {
        crate::platform::Platform::Linux(info) => {
            if info.has_mergerfs {
                format!(
                    "Linux (mergerfs{})",
                    info.mergerfs_version
                        .as_ref()
                        .map(|v| format!(" v{}", v))
                        .unwrap_or_default()
                )
            } else {
                "Linux (mergerfs not installed)".to_string()
            }
        }
        crate::platform::Platform::MacOS(info) => {
            if info.has_fuse_t {
                format!(
                    "macOS (FUSE-T{})",
                    info.fuse_t_version
                        .as_ref()
                        .map(|v| format!(" v{}", v))
                        .unwrap_or_default()
                )
            } else if info.has_macfuse {
                format!(
                    "macOS (macFUSE{})",
                    info.macfuse_version
                        .as_ref()
                        .map(|v| format!(" v{}", v))
                        .unwrap_or_default()
                )
            } else {
                "macOS (no FUSE implementation)".to_string()
            }
        }
        crate::platform::Platform::Unsupported(os) => format!("{} (unsupported)", os),
    };
    println!("  Platform: {}", platform_str);

    Ok(())
}

async fn show_mount_status(
    mount: &Mount,
    resolver: &MountResolver,
    repo_mapping: &RepoMappingManager,
    url: &str,
) -> Result<()> {
    match resolver.resolve_mount(mount) {
        Ok(path) => {
            println!("      Local: {}", path.display());

            // Check if it's auto-managed
            if repo_mapping.is_auto_managed(url)? {
                println!("      Managed: {} (auto-cloned)", "Yes".green());
            } else {
                println!("      Managed: {} (user clone)", "No".yellow());
            }
        }
        Err(e) => {
            // Check if it's just not cloned yet
            if e.to_string().contains("No local repository found") {
                println!(
                    "      Local: {} (will clone on first use)",
                    "Not cloned".yellow()
                );
                println!(
                    "      Tip: Run {} to clone now",
                    format!("thoughts mount clone {url}").cyan()
                );
            } else {
                println!("      Local: {} - {}", "Error".red(), e);
            }
        }
    }
    Ok(())
}
