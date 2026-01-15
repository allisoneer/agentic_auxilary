use crate::config::{RepoConfigManager, RepoMappingManager};
use crate::git::utils::get_control_repo_root;
use anyhow::Result;
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);
    let mapping_mgr = RepoMappingManager::new()?;

    let ds = mgr.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    if ds.references.is_empty() {
        println!("No references configured.");
        println!("Use 'thoughts references add <url>' to add a reference.");
        return Ok(());
    }

    println!("{}", "References:".bold());
    for rm in &ds.references {
        let (org, repo) = crate::config::extract_org_repo_from_url(&rm.remote)
            .unwrap_or_else(|_| ("unknown".to_string(), rm.remote.clone()));

        let status = if let Ok(Some(_)) = mapping_mgr.resolve_url(&rm.remote) {
            "✓ cloned".green()
        } else {
            "✗ not cloned".red()
        };

        println!("  - {}/{} ({})", org, repo, status);
        println!("    {}", rm.remote.dimmed());
        if let Some(desc) = &rm.description {
            println!("      {}", desc.dimmed());
        }
    }

    Ok(())
}
