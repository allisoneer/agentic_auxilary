use crate::config::{MountDirs, RepoConfig, RepoConfigManager};
use crate::git::utils::get_current_repo;
use anyhow::{Context, Result};
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let repo_root = get_current_repo().context("Not in a git repository")?;

    let manager = RepoConfigManager::new(repo_root);

    if manager.load()?.is_some() {
        eprintln!("{}: Repository already has a configuration", "Error".red());
        eprintln!("Edit it with: {}", "thoughts config edit".cyan());
        std::process::exit(1);
    }

    let config = RepoConfig {
        version: "1.0".to_string(),
        mount_dirs: MountDirs::default(),
        requires: vec![],
        rules: vec![],
    };

    manager.save(&config)?;
    println!(
        "{} Created repository configuration at .thoughts/config.json",
        "✓".green()
    );

    Ok(())
}
