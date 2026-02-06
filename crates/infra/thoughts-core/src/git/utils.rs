use crate::error::ThoughtsError;
use crate::repo_identity::RepoIdentity;
use anyhow::Result;
use git2::{Repository, StatusOptions};
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

/// Check if a directory is a git worktree (not a submodule)
///
/// Worktrees have gitdir paths containing "/worktrees/".
/// Submodules have gitdir paths containing "/modules/".
pub fn is_worktree(repo_path: &Path) -> Result<bool> {
    let git_path = repo_path.join(".git");
    if git_path.is_file() {
        let contents = std::fs::read_to_string(&git_path)?;
        if let Some(gitdir_line) = contents
            .lines()
            .find(|l| l.trim_start().starts_with("gitdir:"))
        {
            let gitdir = gitdir_line.trim_start_matches("gitdir:").trim();
            // Worktrees have "/worktrees/" in the path, submodules have "/modules/"
            let is_worktrees = gitdir.contains("/worktrees/");
            let is_modules = gitdir.contains("/modules/");
            if is_worktrees && !is_modules {
                debug!("Found .git file with worktrees path, this is a worktree");
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Get the main repository path for a worktree
///
/// Handles both absolute and relative gitdir paths in the .git file.
pub fn get_main_repo_for_worktree(worktree_path: &Path) -> Result<PathBuf> {
    // For a worktree, we need to find the main repository
    // The .git file in a worktree contains: "gitdir: /path/to/main/.git/worktrees/name"
    // or a relative path like: "gitdir: ../.git/worktrees/name"
    let git_file = worktree_path.join(".git");
    if git_file.is_file() {
        let contents = std::fs::read_to_string(&git_file)?;
        if let Some(gitdir_line) = contents
            .lines()
            .find(|l| l.trim_start().starts_with("gitdir:"))
        {
            let gitdir = gitdir_line.trim_start_matches("gitdir:").trim();
            let mut gitdir_path = PathBuf::from(gitdir);

            // Handle relative paths by resolving against worktree path
            if !gitdir_path.is_absolute() {
                gitdir_path = worktree_path.join(&gitdir_path);
            }

            // Canonicalize to resolve ".." components
            let gitdir_path = std::fs::canonicalize(&gitdir_path).unwrap_or(gitdir_path);

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

/// Get the control repository root (main repo for worktrees, repo root otherwise)
/// This is the authoritative location for .thoughts/config.json and .thoughts-data
pub fn get_control_repo_root(start_path: &Path) -> Result<PathBuf> {
    let repo_root = find_repo_root(start_path)?;
    if is_worktree(&repo_root)? {
        // Best-effort: fall back to repo_root if main cannot be determined
        Ok(get_main_repo_for_worktree(&repo_root).unwrap_or(repo_root))
    } else {
        Ok(repo_root)
    }
}

/// Get the control repository root for the current directory
pub fn get_current_control_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    get_control_repo_root(&cwd)
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

/// Get the canonical identity of a repository's origin remote, if available.
///
/// Returns `Ok(Some(identity))` if the repo has an origin and it parses successfully,
/// `Ok(None)` if the repo has no origin or it can't be parsed, or an error for
/// other failures.
pub fn try_get_origin_identity(repo_path: &Path) -> Result<Option<RepoIdentity>> {
    match get_remote_url(repo_path) {
        Ok(url) => match RepoIdentity::parse(&url) {
            Ok(id) => Ok(Some(id)),
            Err(_) => Ok(None), // URL doesn't parse as a valid identity
        },
        Err(_) => Ok(None), // No origin remote or can't open repo
    }
}

/// Get the current branch name, or "detached" if in detached HEAD state
pub fn get_current_branch(repo_path: &Path) -> Result<String> {
    let repo = Repository::open(repo_path)
        .map_err(|e| anyhow::anyhow!("Failed to open git repository at {:?}: {}", repo_path, e))?;

    let head = repo
        .head()
        .map_err(|e| anyhow::anyhow!("Failed to get HEAD reference: {}", e))?;

    if head.is_branch() {
        Ok(head.shorthand().unwrap_or("unknown").to_string())
    } else {
        Ok("detached".to_string())
    }
}

/// Return true if the repository's working tree has any changes (including untracked)
pub fn is_worktree_dirty(repo: &Repository) -> Result<bool> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    Ok(!statuses.is_empty())
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

    #[test]
    fn test_get_current_branch() {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize repo
        let repo = Repository::init(repo_path).unwrap();

        // Create initial commit so we have a proper HEAD
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        // Should be on master or main (depending on git version)
        let branch = get_current_branch(repo_path).unwrap();
        assert!(branch == "master" || branch == "main");

        // Create and checkout a feature branch
        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        repo.branch("feature-branch", &commit, false).unwrap();
        repo.set_head("refs/heads/feature-branch").unwrap();
        repo.checkout_head(None).unwrap();

        let branch = get_current_branch(repo_path).unwrap();
        assert_eq!(branch, "feature-branch");

        // Test detached HEAD
        let commit_oid = commit.id();
        repo.set_head_detached(commit_oid).unwrap();
        let branch = get_current_branch(repo_path).unwrap();
        assert_eq!(branch, "detached");
    }

    fn initial_commit(repo: &Repository) {
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree_id = {
            let mut idx = repo.index().unwrap();
            idx.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
    }

    #[test]
    fn worktree_dirty_false_when_clean() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        initial_commit(&repo);
        assert!(!is_worktree_dirty(&repo).unwrap());
    }

    #[test]
    fn worktree_dirty_true_for_untracked() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        initial_commit(&repo);

        let fpath = dir.path().join("untracked.txt");
        std::fs::write(&fpath, "hello").unwrap();

        assert!(is_worktree_dirty(&repo).unwrap());
    }

    #[test]
    fn worktree_dirty_true_for_staged() {
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        initial_commit(&repo);

        let fpath = dir.path().join("file.txt");
        {
            let mut f = std::fs::File::create(&fpath).unwrap();
            writeln!(f, "content").unwrap();
        }
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("file.txt")).unwrap();
        idx.write().unwrap();

        assert!(is_worktree_dirty(&repo).unwrap());
    }
}
