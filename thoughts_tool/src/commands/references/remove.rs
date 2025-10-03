use crate::config::RepoConfigManager;
use crate::git::utils::get_control_repo_root;
use anyhow::Result;
use colored::Colorize;

pub async fn execute(url: String) -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);

    let mut cfg = mgr.load_v2_or_bail()?;

    let initial_len = cfg.references.len();
    cfg.references.retain(|u| u != &url);

    if cfg.references.len() == initial_len {
        println!("{}: Reference not found: {}", "Error".red(), url);
        anyhow::bail!("Reference not found");
    }

    mgr.save_v2(&cfg)?;

    println!("{} Removed reference: {}", "✓".green(), url);
    println!(
        "Note: The cloned repository is not deleted. Use 'thoughts mount update' to unmount it."
    );

    Ok(())
}
