use anyhow::Result;
use colored::Colorize;

use crate::config::{PersonalConfigManager, RepoConfigManager};
use crate::git::utils::find_repo_root;

pub async fn execute() -> Result<()> {
    let mut all_valid = true;
    
    // Validate repository config if in a repo
    if let Ok(repo_root) = find_repo_root(&std::env::current_dir()?) {
        print!("Validating repository configuration... ");
        let repo_manager = RepoConfigManager::new(repo_root);
        match repo_manager.load() {
            Ok(Some(_)) => println!("{}", "✓".green()),
            Ok(None) => println!("{}", "not found".yellow()),
            Err(e) => {
                println!("{}: {}", "✗".red(), e);
                all_valid = false;
            }
        }
    }
    
    // Validate personal config
    print!("Validating personal configuration... ");
    match PersonalConfigManager::load() {
        Ok(Some(_)) => println!("{}", "✓".green()),
        Ok(None) => println!("{}", "not found".yellow()),
        Err(e) => {
            println!("{}: {}", "✗".red(), e);
            all_valid = false;
        }
    }
    
    if all_valid {
        println!("\n{} All configurations are valid", "✓".green());
    } else {
        anyhow::bail!("Configuration validation failed");
    }
    
    Ok(())
}