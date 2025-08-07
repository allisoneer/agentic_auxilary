use crate::config::{RepoConfigManager, RepoConfig, MountDirs};
use crate::git::utils::get_current_repo;
use anyhow::{Result, Context};
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let repo_root = get_current_repo()
        .context("Not in a git repository")?;
    
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
    println!("{} Created repository configuration at .thoughts/config.json", "✓".green());
    
    Ok(())
}