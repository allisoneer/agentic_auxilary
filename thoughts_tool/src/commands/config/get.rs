use anyhow::{Result, bail};
use colored::Colorize;

use crate::config::ConfigManager;
use crate::error::ThoughtsError;

pub async fn execute(key: String) -> Result<()> {
    let config_manager = ConfigManager::new()?;
    let config = match config_manager.load() {
        Ok(config) => config,
        Err(ThoughtsError::ConfigNotFound { path: _ }) => {
            eprintln!("{}: No configuration found.", "Error".red());
            std::process::exit(1);
        }
        Err(e) => return Err(e.into()),
    };

    // Parse the key path (e.g., "mounts.personal.source")
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        ["version"] => println!("{}", config.version),
        ["mounts"] => {
            for name in config.mounts.keys() {
                println!("{name}");
            }
        }
        ["mounts", name] => {
            if let Some(mount) = config.mounts.get(*name) {
                println!("type: {}", mount.mount_type());
                match mount {
                    crate::config::Mount::Directory { path, sync } => {
                        println!("path: {}", path.display());
                        println!("sync: {sync}");
                    }
                    crate::config::Mount::Git { url, sync, subpath } => {
                        println!("url: {url}");
                        println!("sync: {sync}");
                        if let Some(sub) = subpath {
                            println!("subpath: {sub}");
                        }
                    }
                }
            } else {
                bail!("Mount '{}' not found", name);
            }
        }
        ["mounts", name, field] => {
            if let Some(mount) = config.mounts.get(*name) {
                match *field {
                    "type" => println!("{}", mount.mount_type()),
                    "sync" => println!("{}", mount.sync_strategy()),
                    "path" => match mount {
                        crate::config::Mount::Directory { path, .. } => {
                            println!("{}", path.display())
                        }
                        crate::config::Mount::Git { .. } => {
                            bail!("Git mounts don't have a path field, use 'url' instead")
                        }
                    },
                    "url" => match mount {
                        crate::config::Mount::Git { url, .. } => println!("{url}"),
                        crate::config::Mount::Directory { .. } => {
                            bail!("Directory mounts don't have a url field, use 'path' instead")
                        }
                    },
                    "subpath" => match mount {
                        crate::config::Mount::Git { subpath, .. } => {
                            if let Some(sub) = subpath {
                                println!("{sub}");
                            }
                        }
                        crate::config::Mount::Directory { .. } => {
                            bail!("Directory mounts don't have a subpath field")
                        }
                    },
                    _ => bail!("Unknown mount field: {}", field),
                }
            } else {
                bail!("Mount '{}' not found", name);
            }
        }
        _ => bail!("Invalid configuration key: {}", key),
    }

    Ok(())
}
