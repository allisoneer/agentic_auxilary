use crate::error::ThoughtsError;
use anyhow::Result;
use git2::Repository;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Get the current repository path, starting from current directory
pub fn get_current_repo() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    find_repo_root(&current_dir)
}

/// Find the repository root from a given path
pub fn find_repo_root(start_path: &Path) -> Result<PathBuf> {
    let repo = Repository::discover(start_path).map_err(|_| ThoughtsError::NotInGitRepo)?;

    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("Repository has no working directory"))?;

    Ok(workdir.to_path_buf())
}

/// Check if a directory is a git worktree
pub fn is_worktree(repo_path: &Path) -> Result<bool> {
    let _repo = Repository::open(repo_path)?;

    // Check if this is a linked worktree by examining the .git file
    let git_path = repo_path.join(".git");
    if git_path.is_file() {
        // If .git is a file, it's a worktree
        debug!("Found .git file, this is a worktree");
        return Ok(true);
    }

    Ok(false)
}

/// Get the main repository path for a worktree
pub fn get_main_repo_for_worktree(worktree_path: &Path) -> Result<PathBuf> {
    let _repo = Repository::open(worktree_path)?;

    // For a worktree, we need to find the main repository
    // The .git file in a worktree contains: "gitdir: /path/to/main/.git/worktrees/name"
    let git_file = worktree_path.join(".git");
    if git_file.is_file() {
        let contents = std::fs::read_to_string(&git_file)?;
        if let Some(gitdir_line) = contents.lines().find(|l| l.starts_with("gitdir:")) {
            let gitdir = gitdir_line.trim_start_matches("gitdir:").trim();
            let gitdir_path = PathBuf::from(gitdir);

            // Navigate from .git/worktrees/name to the main repo
            if let Some(parent) = gitdir_path.parent()
                && let Some(parent_parent) = parent.parent()
                && parent_parent.ends_with(".git")
                && let Some(main_repo) = parent_parent.parent()
            {
                debug!("Found main repo at: {:?}", main_repo);
                return Ok(main_repo.to_path_buf());
            }
        }
    }

    // If we can't determine it from the .git file, fall back to the current repo
    Ok(worktree_path.to_path_buf())
}

/// Check if a path is a git repository
pub fn is_git_repo(path: &Path) -> bool {
    Repository::open(path).is_ok()
}

/// Initialize a new git repository
#[allow(dead_code)]
// TODO(2): Plan initialization architecture for consumer vs source repos
pub fn init_repo(path: &Path) -> Result<Repository> {
    Ok(Repository::init(path)?)
}

/// Get the remote URL for a git repository
pub fn get_remote_url(repo_path: &Path) -> Result<String> {
    let repo = Repository::open(repo_path)
        .map_err(|e| anyhow::anyhow!("Failed to open git repository at {:?}: {}", repo_path, e))?;

    let remote = repo
        .find_remote("origin")
        .map_err(|_| anyhow::anyhow!("No 'origin' remote found"))?;

    remote
        .url()
        .ok_or_else(|| anyhow::anyhow!("Remote 'origin' has no URL"))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_is_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        assert!(!is_git_repo(repo_path));

        Repository::init(repo_path).unwrap();
        assert!(is_git_repo(repo_path));
    }
}
