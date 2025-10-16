use anyhow::{Context, Result};
use git2::{AnnotatedCommit, FetchOptions, Repository};
use std::path::Path;

/// Fast-forward-only pull of the current branch from remote_name (default "origin")
pub fn pull_ff_only(repo_path: &Path, remote_name: &str, branch: Option<&str>) -> Result<()> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("Failed to open repository at {}", repo_path.display()))?;

    let branch = branch.unwrap_or("main");

    // Fetch origin/<branch>
    let mut remote = repo
        .find_remote(remote_name)
        .with_context(|| format!("Remote '{}' not found", remote_name))?;
    let refspec = format!(
        "refs/heads/{b}:refs/remotes/{remote}/{b}",
        b = branch,
        remote = remote_name
    );

    let mut fo = FetchOptions::new();
    // TODO(2): credentials/providers if needed in the future
    remote
        .fetch(&[&refspec], Some(&mut fo), None)
        .with_context(|| "Fetch failed")?;

    // Lookup local and remote refs
    let fetch_head = repo.find_reference(&format!("refs/remotes/{}/{}", remote_name, branch))?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;

    // Try a fast-forward
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
        let mut reference = repo.find_reference(local_ref)?;
        reference.set_target(fetch_commit.id(), "Fast-Forward")?;
        repo.set_head(local_ref)?;
        repo.checkout_head(None)?;
        return Ok(());
    }
    // Non fast-forward - do not merge automatically
    anyhow::bail!("Non fast-forward update required (local changes).")
}
