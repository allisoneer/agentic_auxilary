use anyhow::{Result, bail};
use colored::Colorize;
use std::env;
use std::process::Command;

use crate::config::RepoConfigManager;
use crate::git::utils::get_control_repo_root;
use crate::utils::paths;

pub async fn execute() -> Result<()> {
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let repo_root = get_control_repo_root(&env::current_dir()?)?;
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
    let repo_root = get_control_repo_root(&env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);
    match mgr.peek_config_version()? {
        Some(v) if v == "1.0" => {
            mgr.load()?; // v1 parse triggers validation
            println!("✓ Saved and validated v1 configuration");
        }
        Some(_) => {
            let cfg = mgr.load_v2_or_bail()?;
            let warnings = mgr.save_v2_validated(&cfg)?;
            for w in warnings {
                eprintln!("Warning: {}", w);
            }
            println!("✓ Saved and validated v2 configuration");
        }
        None => bail!("No configuration found after edit"),
    }

    // Update mounts after config change
    println!("\n{} active mounts...", "Updating".cyan());
    crate::mount::auto_mount::update_active_mounts().await?;

    Ok(())
}
