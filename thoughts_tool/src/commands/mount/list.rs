use crate::config::{Mount, RepoConfigManager};
use crate::git::utils::get_control_repo_root;
use crate::mount::{MountResolver, MountSpace};
use anyhow::Result;
use colored::*;
use std::env;

pub async fn execute(verbose: bool) -> Result<()> {
    let repo_root = get_control_repo_root(&env::current_dir()?)?;
    let repo_manager = RepoConfigManager::new(repo_root);
    let resolver = MountResolver::new()?;

    let desired = repo_manager
        .load_desired_state()?
        .ok_or_else(|| anyhow::anyhow!("No repository configuration found"))?;

    // Check if any mounts are configured
    let has_mounts = desired.thoughts_mount.is_some()
        || !desired.context_mounts.is_empty()
        || !desired.references.is_empty();

    if !has_mounts {
        println!("No mounts configured");
        println!("\nAdd mounts with:");
        println!("  {}", "thoughts mount add <path>".cyan());
        return Ok(());
    }

    println!("{}", "Configured mounts:".bold());
    println!();

    // Show thoughts mount if configured
    if let Some(tm) = &desired.thoughts_mount {
        let mount_space = MountSpace::Thoughts;
        println!(
            "{} {}:",
            mount_space.as_str().cyan(),
            "[thoughts workspace]".dimmed()
        );
        let display_url = if let Some(sub) = &tm.subpath {
            format!("{}:{}", tm.remote, sub)
        } else {
            tm.remote.clone()
        };
        println!("  URL: {display_url}");
        println!("  Sync: {}", tm.sync);

        if verbose {
            let mount = Mount::Git {
                url: tm.remote.clone(),
                sync: tm.sync,
                subpath: tm.subpath.clone(),
            };
            match resolver.resolve_mount(&mount) {
                Ok(path) => println!("  Path: {}", path.display()),
                Err(_) => println!("  Path: {} (not cloned)", "not cloned".yellow()),
            }
        }
        println!();
    }

    // Show context mounts
    if !desired.context_mounts.is_empty() {
        println!("{}", "Context mounts:".yellow());
        for cm in &desired.context_mounts {
            let mount_space = MountSpace::Context(cm.mount_path.clone());
            println!(
                "  {} {}:",
                mount_space.as_str().cyan(),
                format!("[{}]", cm.mount_path).dimmed()
            );
            let display_url = if let Some(sub) = &cm.subpath {
                format!("{}:{}", cm.remote, sub)
            } else {
                cm.remote.clone()
            };
            println!("    URL: {display_url}");
            println!("    Sync: {}", cm.sync);

            if verbose {
                let mount = Mount::Git {
                    url: cm.remote.clone(),
                    sync: cm.sync,
                    subpath: cm.subpath.clone(),
                };
                match resolver.resolve_mount(&mount) {
                    Ok(path) => println!("    Path: {}", path.display()),
                    Err(_) => println!(
                        "    Path: {} (will clone on first use)",
                        "not cloned".yellow()
                    ),
                }
            }
            println!();
        }
    }

    // Show references
    if !desired.references.is_empty() {
        println!("{}", "References:".green());
        println!("  {} URLs configured", desired.references.len());
        println!("  Use {} to see details", "thoughts references list".cyan());
    }

    Ok(())
}
