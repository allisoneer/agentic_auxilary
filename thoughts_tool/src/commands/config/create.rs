use crate::config::RepoConfigManager;
use crate::git::utils::get_current_control_repo_root;
use crate::utils::paths;
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let repo_root = get_current_control_repo_root().context("Not in a git repository")?;

    let mgr = RepoConfigManager::new(repo_root.clone());

    let config_path = paths::get_repo_config_path(&repo_root);
    if config_path.exists() {
        eprintln!("{}: Repository already has a configuration", "Error".red());
        eprintln!("Edit it with: {}", "thoughts config edit".cyan());
        std::process::exit(1);
    }

    let _ = mgr.ensure_v2_default()?;
    println!(
        "{} Created v2 repository configuration at .thoughts/config.json",
        "✓".green()
    );

    Ok(())
}
