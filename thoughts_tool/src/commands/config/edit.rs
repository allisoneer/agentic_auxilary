use anyhow::{Result, bail};
use colored::Colorize;
use std::env;
use std::process::Command;

use crate::config::RepoConfigManager;
use crate::git::utils::find_repo_root;
use crate::utils::paths;

pub async fn execute() -> Result<()> {
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let repo_root = find_repo_root(&env::current_dir()?)?;
    let config_path = paths::get_repo_config_path(&repo_root);

    if !config_path.exists() {
        bail!("No repository configuration found. Run 'thoughts init' first.");
    }

    // Open in editor
    let status = Command::new(&editor).arg(&config_path).status()?;

    if !status.success() {
        bail!("Editor exited with error");
    }

    // Validate after editing
    let repo_root = find_repo_root(&env::current_dir()?)?;
    let repo_manager = RepoConfigManager::new(repo_root);
    repo_manager.load()?;
    println!("{} Repository configuration is valid", "✓".green());

    // Update mounts after config change
    println!("\n{} active mounts...", "Updating".cyan());
    crate::mount::auto_mount::update_active_mounts().await?;

    Ok(())
}
