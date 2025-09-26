use anyhow::Result;
use colored::Colorize;

use crate::config::RepoConfigManager;
use crate::git::utils::find_repo_root;

pub async fn execute() -> Result<()> {
    // Validate repository config
    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    print!("Validating repository configuration... ");
    let repo_manager = RepoConfigManager::new(repo_root);
    match repo_manager.load() {
        Ok(Some(_)) => {
            println!("{}", "✓".green());
            println!("\n{} Repository configuration is valid", "✓".green());
        }
        Ok(None) => {
            println!("{}", "not found".yellow());
            anyhow::bail!("No repository configuration found. Run 'thoughts init' first.");
        }
        Err(e) => {
            println!("{}: {}", "✗".red(), e);
            anyhow::bail!("Configuration validation failed: {}", e);
        }
    }

    Ok(())
}
