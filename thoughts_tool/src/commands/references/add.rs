use crate::config::{ReferenceEntry, RepoConfigManager};
use crate::git::utils::get_control_repo_root;
use anyhow::Result;
use colored::Colorize;

pub async fn execute(url: String) -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);

    let mut cfg = mgr.ensure_v2_default()?;

    // Check if URL already exists (compare with Simple variant)
    let entry = ReferenceEntry::Simple(url.clone());
    if cfg.references.contains(&entry) {
        println!("{}: Reference already exists", "Note".yellow());
        return Ok(());
    }

    cfg.references.push(entry);
    mgr.save_v2(&cfg)?;

    println!("{} Added reference: {}", "✓".green(), url);
    println!("Run 'thoughts references sync' to clone and mount it.");

    Ok(())
}
