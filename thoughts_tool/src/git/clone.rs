use anyhow::{Context, Result};
use colored::*;
use std::path::PathBuf;

use crate::git::progress::InlineProgress;

pub struct CloneOptions {
    pub url: String,
    pub target_path: PathBuf,
    pub branch: Option<String>,
}

pub fn clone_repository(options: &CloneOptions) -> Result<()> {
    println!("{} {}", "Cloning".green(), options.url);
    println!("  to: {}", options.target_path.display());

    // Ensure parent directory exists
    if let Some(parent) = options.target_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create clone directory")?;
    }

    // Ensure target directory is empty
    if options.target_path.exists() {
        let entries = std::fs::read_dir(&options.target_path)?;
        if entries.count() > 0 {
            anyhow::bail!(
                "Target directory is not empty: {}",
                options.target_path.display()
            );
        }
    }

    // SAFETY: progress handler is lock-free and alloc-minimal
    unsafe {
        gix::interrupt::init_handler(1, || {}).ok();
    }

    let url = gix::url::parse(options.url.as_str().into())
        .with_context(|| format!("Invalid repository URL: {}", options.url))?;

    let mut prepare =
        gix::prepare_clone(url, &options.target_path).context("Failed to prepare clone")?;

    if let Some(branch) = &options.branch {
        prepare = prepare
            .with_ref_name(Some(branch.as_str()))
            .context("Failed to set target branch")?;
    }

    let (mut checkout, _fetch_outcome) = prepare
        .fetch_then_checkout(
            InlineProgress::new("progress"),
            &gix::interrupt::IS_INTERRUPTED,
        )
        .context("Fetch failed")?;

    let (_repo, _outcome) = checkout
        .main_worktree(
            InlineProgress::new("checkout"),
            &gix::interrupt::IS_INTERRUPTED,
        )
        .context("Checkout failed")?;

    println!("\n{} Clone completed successfully", "✓".green());
    Ok(())
}
