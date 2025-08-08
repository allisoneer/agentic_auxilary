use crate::config::RepoMappingManager;
use crate::git::clone::clone_repository;
use crate::utils::paths::expand_path;
use anyhow::{Context, Result, bail};
use colored::*;
use std::path::PathBuf;

pub async fn execute(url: String, path: Option<PathBuf>) -> Result<()> {
    println!("{} repository...", "Cloning".green());

    let mut repo_mapping = RepoMappingManager::new()?;

    // Check if already mapped
    if let Some(existing_path) = repo_mapping.resolve_url(&url)? {
        println!(
            "{}: Repository already cloned at {}",
            "Info".yellow(),
            existing_path.display()
        );

        if path.is_some() {
            bail!("Repository already mapped. Remove the existing mapping first.");
        }

        return Ok(());
    }

    // Determine clone path
    let (clone_path, auto_managed) = if let Some(p) = path {
        let expanded = expand_path(&p)?;

        // Validate parent directory exists
        if let Some(parent) = expanded.parent() {
            if !parent.exists() {
                bail!("Parent directory does not exist: {}", parent.display());
            }
        }

        (expanded, false) // User-specified path is not auto-managed
    } else {
        (RepoMappingManager::get_default_clone_path(&url)?, true) // Default path is auto-managed
    };

    // Check if path already exists
    if clone_path.exists() {
        bail!("Path already exists: {}", clone_path.display());
    }

    // Clone repository
    println!("Cloning to: {}", clone_path.display());
    let clone_opts = crate::git::clone::CloneOptions {
        url: url.clone(),
        target_path: clone_path.clone(),
        shallow: false,
        branch: None,
    };
    crate::git::clone::clone_repository(&clone_opts).context("Failed to clone repository")?;

    // Add mapping
    repo_mapping.add_mapping(url.clone(), clone_path.clone(), auto_managed)?;

    println!("{} Repository cloned successfully", "✓".green());
    println!("\nYou can now use this repository in mounts:");
    println!("  {}", format!("thoughts mount add {}", url).cyan());

    Ok(())
}
