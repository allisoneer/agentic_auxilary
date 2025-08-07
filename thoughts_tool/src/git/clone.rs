use anyhow::{Result, Context};
use git2::{Repository, FetchOptions, Progress, RemoteCallbacks};
use std::path::{Path, PathBuf};
use colored::*;

pub struct CloneOptions {
    pub url: String,
    pub target_path: PathBuf,
    pub shallow: bool,
    pub branch: Option<String>,
}

pub fn clone_repository(options: &CloneOptions) -> Result<()> {
    println!("{} {}", "Cloning".green(), options.url);
    println!("  to: {}", options.target_path.display());
    
    // Ensure parent directory exists
    if let Some(parent) = options.target_path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create clone directory")?;
    }
    
    // Ensure target doesn't exist or is empty
    if options.target_path.exists() {
        let entries = std::fs::read_dir(&options.target_path)?;
        if entries.count() > 0 {
            anyhow::bail!("Target directory is not empty: {}", options.target_path.display());
        }
    }
    
    // Set up clone
    let mut builder = git2::build::RepoBuilder::new();
    let mut fetch_opts = FetchOptions::new();
    let mut callbacks = RemoteCallbacks::new();
    
    // Progress callback
    callbacks.transfer_progress(|stats: Progress| {
        let received = stats.received_objects();
        let total = stats.total_objects();
        
        if total > 0 {
            let percent = (received as f32 / total as f32) * 100.0;
            print!("\r  {}: {}/{} objects ({:.1}%)", 
                "Progress".cyan(),
                received,
                total,
                percent
            );
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
        true
    });
    
    fetch_opts.remote_callbacks(callbacks);
    builder.fetch_options(fetch_opts);
    
    if let Some(branch) = &options.branch {
        builder.branch(branch);
    }
    
    // Perform clone
    builder.clone(&options.url, &options.target_path)
        .context("Failed to clone repository")?;
    
    println!("\n✓ Clone completed successfully");
    Ok(())
}

pub fn is_valid_clone_target(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(true);
    }
    
    if path.is_dir() {
        let entries = std::fs::read_dir(path)?;
        Ok(entries.count() == 0)
    } else {
        Ok(false)
    }
}