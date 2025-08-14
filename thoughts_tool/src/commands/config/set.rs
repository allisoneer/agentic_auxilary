use anyhow::{Context, Result, bail};
use colored::Colorize;

use crate::config::{ConfigManager, SyncStrategy};
use crate::error::ThoughtsError;

pub async fn execute(key: String, value: String) -> Result<()> {
    let config_manager = ConfigManager::new()?;
    let mut config = match config_manager.load() {
        Ok(config) => config,
        Err(ThoughtsError::ConfigNotFound { path: _ }) => {
            eprintln!("{}: No configuration found.", "Error".red());
            eprintln!("Run '{}' to initialize first.", "thoughts init".cyan());
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    };

    // Create backup before modifying
    config_manager.backup().context("Failed to create backup")?;

    // Parse the key path
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        ["mounts", name, "sync"] => {
            if let Some(mount) = config.mounts.get_mut(*name) {
                let new_sync = value
                    .parse::<SyncStrategy>()
                    .context("Invalid sync strategy. Use: none or auto")?;
                match mount {
                    crate::config::Mount::Directory { sync, .. } => *sync = new_sync,
                    crate::config::Mount::Git { sync, .. } => *sync = new_sync,
                }
                println!("Set mount '{name}' sync strategy to '{value}'");
            } else {
                bail!("Mount '{}' not found", name);
            }
        }
        ["mounts", name, "url"] => {
            if let Some(mount) = config.mounts.get_mut(*name) {
                match mount {
                    crate::config::Mount::Git { url, .. } => {
                        *url = value.clone();
                        println!("Set mount '{name}' url to '{value}'");
                    }
                    crate::config::Mount::Directory { .. } => {
                        bail!("URL can only be set for git mounts");
                    }
                }
            } else {
                bail!("Mount '{}' not found", name);
            }
        }
        _ => bail!(
            "Cannot set '{}'. Only specific mount fields can be modified.",
            key
        ),
    }

    // Validate and save
    config_manager.validate(&config)?;
    config_manager.save(&config)?;

    println!("{} Configuration updated successfully!", "✓".green());

    Ok(())
}
