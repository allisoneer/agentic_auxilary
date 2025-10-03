use crate::config::{RepoConfigManager, RepoMappingManager};
use crate::git::clone::{CloneOptions, clone_repository};
use crate::git::utils::get_control_repo_root;
use anyhow::Result;
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);
    let mut mapping_mgr = RepoMappingManager::new()?;

    let ds = mgr.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    if ds.references.is_empty() {
        println!("No references configured.");
        return Ok(());
    }

    let mut cloned_count = 0;
    let mut skipped_count = 0;

    for url in &ds.references {
        // Check if already cloned
        if mapping_mgr.resolve_url(url)?.is_some() {
            println!("{} {} (already cloned)", "→".dimmed(), url.dimmed());
            skipped_count += 1;
            continue;
        }

        // Clone to default path
        let default_path = RepoMappingManager::get_default_clone_path(url)?;

        match clone_repository(&CloneOptions {
            url: url.clone(),
            target_path: default_path.clone(),
            branch: None,
        }) {
            Ok(_) => {
                // Add mapping
                mapping_mgr.add_mapping(url.clone(), default_path, true)?;
                println!("{} Cloned {}", "✓".green(), url);
                cloned_count += 1;
            }
            Err(e) => {
                println!("{} Failed to clone {}: {}", "✗".red(), url, e);
            }
        }
    }

    println!();
    println!("Cloned: {}, Skipped: {}", cloned_count, skipped_count);

    if cloned_count > 0 {
        println!("Run 'thoughts mount update' to mount the new references.");
    }

    Ok(())
}
