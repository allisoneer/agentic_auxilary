use anyhow::{Context, Result};
use git2::{AnnotatedCommit, Repository, StatusOptions};
use std::path::Path;
use std::process::Command;

/// Fetch from remote using system git (which uses system SSH, triggers 1Password prompts)
fn fetch_with_shell_git(repo_path: &Path, remote_name: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(["fetch", remote_name])
        .status()
        .context("Failed to spawn git fetch")?;

    if !status.success() {
        anyhow::bail!("git fetch failed with exit code {:?}", status.code());
    }

    Ok(())
}

fn worktree_is_dirty(repo: &Repository) -> Result<bool> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    Ok(!statuses.is_empty())
}

/// Fast-forward-only pull of the current branch from remote_name (default "origin")
/// Uses shell git for fetch (to trigger 1Password SSH prompts) and git2 for fast-forward
pub fn pull_ff_only(repo_path: &Path, remote_name: &str, branch: Option<&str>) -> Result<()> {
    // First check if remote exists
    {
        let repo = Repository::open(repo_path)
            .with_context(|| format!("Failed to open repository at {}", repo_path.display()))?;
        if repo.find_remote(remote_name).is_err() {
            // No remote - nothing to fetch
            return Ok(());
        }
    }

    let branch = branch.unwrap_or("main");

    // Fetch using shell git (uses system SSH, triggers 1Password)
    fetch_with_shell_git(repo_path, remote_name).context("Fetch failed")?;

    // Re-open repository to see the fetched refs
    let repo = Repository::open(repo_path)
        .with_context(|| format!("Failed to re-open repository at {}", repo_path.display()))?;

    // Now do the fast-forward using git2
    let remote_ref = format!("refs/remotes/{}/{}", remote_name, branch);
    let fetch_head = match repo.find_reference(&remote_ref) {
        Ok(r) => r,
        Err(_) => {
            // Remote branch doesn't exist yet
            return Ok(());
        }
    };
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;

    try_fast_forward(&repo, &format!("refs/heads/{}", branch), &fetch_commit)?;
    Ok(())
}

fn try_fast_forward(
    repo: &Repository,
    local_ref: &str,
    fetch_commit: &AnnotatedCommit,
) -> Result<()> {
    let analysis = repo.merge_analysis(&[fetch_commit])?;
    if analysis.0.is_up_to_date() {
        return Ok(());
    }
    if analysis.0.is_fast_forward() {
        // Safety gate: never force-checkout over local changes
        if worktree_is_dirty(repo)? {
            anyhow::bail!(
                "Cannot fast-forward: working tree has uncommitted changes. Please commit or stash before pulling."
            );
        }
        let mut reference = repo.find_reference(local_ref)?;
        reference.set_target(fetch_commit.id(), "Fast-Forward")?;
        repo.set_head(local_ref)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        return Ok(());
    }
    anyhow::bail!(
        "Non fast-forward update required (local and remote have diverged; rebase or merge needed)."
    )
}
