use anyhow::Result;
use colored::Colorize;

use crate::config::RepoConfigManager;
use crate::git::utils::get_control_repo_root;

pub async fn execute() -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    print!("Validating repository configuration... ");
    let mgr = RepoConfigManager::new(repo_root);

    match mgr.peek_config_version()? {
        None => {
            anyhow::bail!("No repository configuration found. Run 'thoughts init' first.")
        }
        Some(v) if v == "1.0" => {
            anyhow::bail!(
                "V1 configuration is no longer supported. Please reinitialize with 'thoughts init'."
            );
        }
        Some(_) => {
            let cfg = mgr.load_v2_or_bail()?;
            let warnings = mgr.validate_v2_hard(&cfg)?;
            println!("{}", "✓".green());
            println!("{} v2 configuration is valid", "✓".green());
            if !warnings.is_empty() {
                println!("\nWarnings:");
                for w in warnings {
                    println!("  - {}", w);
                }
            }
        }
    }

    Ok(())
}
