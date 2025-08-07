use crate::config::{MountMerger, MountSource, Mount};
use crate::git::utils::find_repo_root;
use crate::mount::MountResolver;
use anyhow::Result;
use colored::*;
use std::env;

pub async fn execute(verbose: bool) -> Result<()> {
    let repo_root = find_repo_root(&env::current_dir()?)?;
    let merger = MountMerger::new(repo_root);
    let resolver = MountResolver::new()?;
    
    let all_mounts = merger.get_all_mounts().await?;
    
    if all_mounts.is_empty() {
        println!("No mounts configured");
        println!("\nAdd mounts with:");
        println!("  {} (repository mount)", "thoughts mount add <path>".cyan());
        println!("  {} (personal mount)", "thoughts mount add <path> --personal".cyan());
        return Ok(());
    }
    
    println!("{}", "Configured mounts:".bold());
    println!();
    
    for (name, (mount, source)) in &all_mounts {
        println!("{}:", name.cyan());
        
        // Show source
        let source_indicator = match source {
            MountSource::Repository => "[repo]",
            MountSource::Personal => "[personal]",
            MountSource::Pattern => "[pattern]",
        };
        println!("  Type: {}", source_indicator.dimmed());
        
        // Show mount details
        match mount {
            Mount::Git { url, sync, subpath } => {
                let display_url = if let Some(sub) = subpath {
                    format!("{}:{}", url, sub)
                } else {
                    url.clone()
                };
                println!("  URL: {}", display_url);
                println!("  Sync: {}", sync);
                
                if verbose {
                    // Show resolved path if available
                    match resolver.resolve_mount(mount) {
                        Ok(path) => println!("  Path: {}", path.display()),
                        Err(_) => println!("  Path: {} (will clone on first use)", "not cloned".yellow()),
                    }
                }
            }
            Mount::Directory { path, sync } => {
                println!("  Path: {}", path.display());
                println!("  Sync: {}", sync);
            }
        }
        println!();
    }
    
    Ok(())
}