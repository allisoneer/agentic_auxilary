use anyhow::{Context, Result};
use colored::*;
use std::path::{Path, PathBuf};

use crate::git::progress::InlineProgress;
use crate::git::utils::{get_remote_url, is_git_repo};
use crate::repo_identity::RepoIdentity;
use crate::utils::locks::FileLock;

pub struct CloneOptions {
    pub url: String,
    pub target_path: PathBuf,
    pub branch: Option<String>,
}

/// Get the clone lock path for a target directory.
///
/// Lock file is placed adjacent to the target: `.{dirname}.clone.lock`
fn clone_lock_path(target_path: &Path) -> Result<PathBuf> {
    let parent = target_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory for clone path"))?;
    let name = target_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("No directory name for clone path"))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.clone.lock")))
}

pub fn clone_repository(options: &CloneOptions) -> Result<()> {
    // Ensure parent directory exists (needed for lock file)
    if let Some(parent) = options.target_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create clone directory")?;
    }

    // Acquire per-target clone lock to prevent concurrent clones
    let _lock = FileLock::lock_exclusive(clone_lock_path(&options.target_path)?)?;

    // Idempotent check: if target is already a git repo, verify identity matches
    if options.target_path.exists() && is_git_repo(&options.target_path) {
        let existing_url = get_remote_url(&options.target_path)?;
        let want = RepoIdentity::parse(&options.url)?.canonical_key();
        let have = RepoIdentity::parse(&existing_url)?.canonical_key();

        if want == have {
            println!(
                "{} Already cloned: {}",
                "✓".green(),
                options.target_path.display()
            );
            return Ok(());
        }

        anyhow::bail!(
            "Clone target already contains a different repository:\n\
             \n  target: {}\n  requested: {}\n  existing origin: {}",
            options.target_path.display(),
            options.url,
            existing_url
        );
    }

    // Ensure target directory is empty (if it exists but isn't a git repo)
    if options.target_path.exists() {
        let entries = std::fs::read_dir(&options.target_path).with_context(|| {
            format!(
                "Failed to read target directory: {}",
                options.target_path.display()
            )
        })?;
        if entries.count() > 0 {
            anyhow::bail!(
                "Target directory exists but is not a git repo (and is not empty): {}",
                options.target_path.display()
            );
        }
    }

    println!("{} {}", "Cloning".green(), options.url);
    println!("  to: {}", options.target_path.display());

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

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use tempfile::TempDir;

    fn create_git_repo_with_origin(dir: &std::path::Path, origin_url: &str) {
        let repo = Repository::init(dir).unwrap();
        repo.remote("origin", origin_url).unwrap();
    }

    #[test]
    fn test_idempotent_clone_same_identity() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("repo");
        std::fs::create_dir_all(&target).unwrap();

        // Create a git repo with matching origin (SSH format)
        create_git_repo_with_origin(&target, "git@github.com:org/repo.git");

        // Try to "clone" with HTTPS URL (same canonical identity)
        let options = CloneOptions {
            url: "https://github.com/org/repo".to_string(),
            target_path: target.clone(),
            branch: None,
        };

        // Should succeed without actually cloning (idempotent)
        let result = clone_repository(&options);
        assert!(result.is_ok(), "Expected success for matching identity");
    }

    #[test]
    fn test_clone_fails_for_different_identity() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("repo");
        std::fs::create_dir_all(&target).unwrap();

        // Create a git repo with different origin
        create_git_repo_with_origin(&target, "git@github.com:alice/utils.git");

        // Try to clone a different repo
        let options = CloneOptions {
            url: "https://github.com/bob/utils.git".to_string(),
            target_path: target.clone(),
            branch: None,
        };

        let result = clone_repository(&options);
        assert!(result.is_err(), "Expected error for different identity");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("different repository"),
            "Error should mention different repository: {}",
            err
        );
    }

    #[test]
    fn test_clone_fails_for_non_git_non_empty() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("repo");
        std::fs::create_dir_all(&target).unwrap();

        // Create a non-git file in the directory
        std::fs::write(target.join("file.txt"), "hello").unwrap();

        let options = CloneOptions {
            url: "https://github.com/org/repo.git".to_string(),
            target_path: target.clone(),
            branch: None,
        };

        let result = clone_repository(&options);
        assert!(result.is_err(), "Expected error for non-empty non-git dir");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not a git repo"),
            "Error should mention not a git repo: {}",
            err
        );
    }
}
