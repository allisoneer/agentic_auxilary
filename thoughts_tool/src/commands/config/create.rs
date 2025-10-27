use crate::config::RepoConfigManager;
use crate::git::utils::get_current_control_repo_root;
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let repo_root = get_current_control_repo_root().context("Not in a git repository")?;

    let mgr = RepoConfigManager::new(repo_root);

    if mgr.peek_config_version()?.is_some() {
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
