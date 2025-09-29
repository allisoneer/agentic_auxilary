use crate::config::RepoConfigManager;
use crate::git::utils::find_repo_root;
use anyhow::Result;
use colored::Colorize;

pub async fn execute(url: String) -> Result<()> {
    let repo_root = find_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);

    let mut cfg = mgr.ensure_v2_default()?;

    // Check if URL already exists
    if cfg.references.contains(&url) {
        println!("{}: Reference already exists", "Note".yellow());
        return Ok(());
    }

    cfg.references.push(url.clone());
    mgr.save_v2(&cfg)?;

    println!("{} Added reference: {}", "✓".green(), url);
    println!("Run 'thoughts references sync' to clone and mount it.");

    Ok(())
}
