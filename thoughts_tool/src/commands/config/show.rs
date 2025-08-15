use anyhow::Result;
use colored::Colorize;
use serde_json;

use crate::config::{PersonalConfigManager, RepoConfigManager};
use crate::git::utils::find_repo_root;

pub async fn execute(json: bool, personal: bool) -> Result<()> {
    if personal {
        // Show personal config from ~/.thoughts/config.json
        if let Some(config) = PersonalConfigManager::load()? {
            if json {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else {
                println!("{}", "Personal Configuration".bold());
                println!();

                // Show patterns
                if !config.patterns.is_empty() {
                    println!("{}:", "Patterns".cyan());
                    for pattern in &config.patterns {
                        println!("  {} - {}", pattern.match_remote, pattern.description);
                        for mount in &pattern.personal_mounts {
                            println!("    {} → {}", mount.mount_path, mount.remote);
                        }
                    }
                    println!();
                }

                // Show repository-specific mounts
                if !config.repository_mounts.is_empty() {
                    println!("{}:", "Repository Mounts".cyan());
                    for (repo, mounts) in &config.repository_mounts {
                        println!("  {repo}:");
                        for mount in mounts {
                            println!("    {} → {}", mount.mount_path, mount.remote);
                        }
                    }
                }
            }
        } else {
            println!("No personal configuration found at ~/.thoughts/config.json");
            println!("Personal configuration will be created when you add personal mounts.");
        }
    } else {
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
    }

    Ok(())
}
