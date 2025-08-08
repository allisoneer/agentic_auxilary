use crate::config::{Mount, MountMerger, MountSource, RepoMappingManager};
use crate::git::utils::find_repo_root;
use crate::mount::{MountResolver, get_mount_manager};
use crate::platform::detect_platform;
use anyhow::Result;
use colored::*;
use std::env;

pub async fn execute(_detailed: bool) -> Result<()> {
    let repo_root = match find_repo_root(&env::current_dir()?) {
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

    let merger = MountMerger::new(repo_root.clone());
    let platform_info = detect_platform()?;
    let mount_manager = get_mount_manager(&platform_info)?;
    let resolver = MountResolver::new()?;
    let repo_mapping = RepoMappingManager::new()?;

    // Get all configured mounts with sources
    let all_mounts = merger.get_all_mounts().await?;
    let active_mounts = mount_manager.list_mounts().await?;

    println!("{}", "Thoughts Tool Status".bold().cyan());
    println!("{}", "===================".cyan());
    println!();

    // Repository info
    println!("{}", "Repository:".bold());
    println!("  Path: {}", repo_root.display());
    if let Ok(url) = crate::git::utils::get_remote_url(&repo_root) {
        println!("  Remote: {}", url);
    }
    println!();

    // Mount status
    println!("{}", "Mounts:".bold());

    if all_mounts.is_empty() {
        println!("  No mounts configured");
        println!();
        println!("  Add mounts with:");
        println!(
            "    {} (repository mount)",
            "thoughts mount add <url>".cyan()
        );
        println!(
            "    {} (personal mount)",
            "thoughts mount add <url> --personal".cyan()
        );
    } else {
        for (name, (mount, source)) in &all_mounts {
            println!("\n  {}:", name.cyan().bold());

            // Show source with description
            let (source_label, source_color) = match source {
                MountSource::Repository => ("Repository", "green"),
                MountSource::Personal => ("Personal", "blue"),
                MountSource::Pattern => ("Pattern", "magenta"),
            };

            let source_str = match source {
                MountSource::Repository => format!("{} (shared with team)", source_label),
                MountSource::Personal => format!("{} (private to you)", source_label),
                MountSource::Pattern => format!("{} (matched by rule)", source_label),
            };

            println!("    Source: {}", source_str.color(source_color));

            // Show mount details
            match mount {
                Mount::Git { url, subpath, sync } => {
                    let display_url = if let Some(sub) = subpath {
                        format!("{}:{}", url, sub)
                    } else {
                        url.clone()
                    };
                    println!("    URL: {}", display_url);
                    println!("    Sync: {}", format!("{:?}", sync).dimmed());

                    // Show local path and clone status
                    match resolver.resolve_mount(mount) {
                        Ok(path) => {
                            println!("    Local: {}", path.display());

                            // Check if it's auto-managed
                            if repo_mapping.is_auto_managed(url)? {
                                println!("    Managed: {} (auto-cloned)", "Yes".green());
                            } else {
                                println!("    Managed: {} (user clone)", "No".yellow());
                            }
                        }
                        Err(e) => {
                            // Check if it's just not cloned yet
                            if e.to_string().contains("No local repository found") {
                                println!(
                                    "    Local: {} (will clone on first use)",
                                    "Not cloned".yellow()
                                );
                                println!(
                                    "    Tip: Run {} to clone now",
                                    format!("thoughts mount clone {}", url).cyan()
                                );
                            } else {
                                println!("    Local: {} - {}", "Error".red(), e);
                            }
                        }
                    }
                }
                Mount::Directory { path, sync } => {
                    println!("    Type: Local directory");
                    println!("    Path: {}", path.display());
                    println!("    Sync: {}", format!("{:?}", sync).dimmed());
                }
            }

            // Show mount status
            let is_mounted = active_mounts.iter().any(|m| {
                m.target
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == name)
                    .unwrap_or(false)
            });

            if is_mounted {
                println!("    Status: {} ✓", "Mounted".green().bold());
            } else {
                println!("    Status: {} ✗", "Not mounted".red().bold());
                println!("    Tip: Run {} to mount", "thoughts mount update".cyan());
            }
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
    let platform = if cfg!(target_os = "linux") {
        "Linux (mergerfs)"
    } else if cfg!(target_os = "macos") {
        "macOS (FUSE-T)"
    } else {
        "Unsupported"
    };
    println!("  Platform: {}", platform);

    Ok(())
}
