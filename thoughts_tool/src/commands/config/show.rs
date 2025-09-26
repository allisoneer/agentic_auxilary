use anyhow::Result;
use colored::Colorize;
use serde_json;

use crate::config::RepoConfigManager;
use crate::git::utils::find_repo_root;

pub async fn execute(json: bool) -> Result<()> {
    // Show repository config from .thoughts/config.json
    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let repo_manager = RepoConfigManager::new(repo_root);

    if let Some(config) = repo_manager.load()? {
        if json {
            println!("{}", serde_json::to_string_pretty(&config)?);
        } else {
            println!("{}", "Repository Configuration".bold());
            println!();
            println!("Version: {}", config.version);
            println!();

            if !config.requires.is_empty() {
                println!("{}:", "Required Mounts".cyan());
                for mount in &config.requires {
                    let display = if let Some(sub) = &mount.subpath {
                        format!("{}:{}", mount.remote, sub)
                    } else {
                        mount.remote.clone()
                    };
                    println!("  {} → {}", mount.mount_path, display);
                }
            } else {
                println!("No mounts configured");
            }
        }
    } else {
        println!("No repository configuration found");
        println!("Run {} to initialize", "thoughts init".cyan());
    }

    Ok(())
}
