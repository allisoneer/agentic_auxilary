use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::env;
use std::process::Command;

use crate::config::{PersonalConfigManager, RepoConfigManager};
use crate::git::utils::find_repo_root;
use crate::utils::paths;

pub async fn execute(personal: bool) -> Result<()> {
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    
    let config_path = if personal {
        paths::get_personal_config_path()?
    } else {
        let repo_root = find_repo_root(&env::current_dir()?)?;
        paths::get_repo_config_path(&repo_root)
    };
    
    if !config_path.exists() {
        if personal {
            bail!("No personal configuration found. Run 'thoughts mount add <url> --personal' to create one.");
        } else {
            bail!("No repository configuration found. Run 'thoughts init' first.");
        }
    }
    
    // Open in editor
    let status = Command::new(&editor)
        .arg(&config_path)
        .status()?;
    
    if !status.success() {
        bail!("Editor exited with error");
    }
    
    // Validate after editing
    if personal {
        PersonalConfigManager::load()?;
        println!("{} Personal configuration is valid", "✓".green());
    } else {
        let repo_root = find_repo_root(&env::current_dir()?)?;
        let repo_manager = RepoConfigManager::new(repo_root);
        repo_manager.load()?;
        println!("{} Repository configuration is valid", "✓".green());
    }
    
    // Update mounts after config change
    println!("\n{} active mounts...", "Updating".cyan());
    crate::mount::auto_mount::update_active_mounts().await?;
    
    Ok(())
}