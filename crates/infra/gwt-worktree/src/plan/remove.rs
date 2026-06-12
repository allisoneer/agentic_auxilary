use crate::error::Error;
use crate::error::Result;
use crate::pr::PullRequestLookup;
use crate::pr::PullRequestState;
use crate::repo::ControlRepo;
use crate::types::BranchName;
use crate::worktree::is_worktree_dirty;
use crate::worktree::list_worktrees;
use git2::Repository;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveRequest {
    pub branch: BranchName,
    pub force: bool,
    pub allow_outside_base: bool,
    pub delete_remote: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovePlan {
    pub branch: BranchName,
    pub worktree_path: PathBuf,
    pub is_main: bool,
    pub dirty: bool,
    pub locked: bool,
    pub prunable: bool,
    pub outside_base: bool,
    pub force: bool,
    pub allow_outside_base: bool,
    pub delete_remote: bool,
    pub requires_remote_deleter: bool,
    pub pr_state: Option<PullRequestState>,
}

pub fn plan_remove(
    control_repo: &ControlRepo,
    request: &RemoveRequest,
    pr_lookup: Option<&dyn PullRequestLookup>,
) -> Result<RemovePlan> {
    let entries = list_worktrees(control_repo)?;
    let entry = entries
        .into_iter()
        .find(|entry| entry.branch.as_deref() == Some(request.branch.as_str()))
        .ok_or_else(|| Error::BranchNotFound(request.branch.to_string()))?;
    let dirty = if entry.is_main {
        false
    } else {
        is_worktree_dirty(&Repository::open(&entry.path)?)?
    };
    let outside_base = !entry.path.starts_with(&control_repo.worktree_base);
    let pr_state = pr_lookup
        .map(|lookup| lookup.lookup_pull_request_state(&request.branch))
        .transpose()?;

    Ok(RemovePlan {
        branch: request.branch.clone(),
        worktree_path: entry.path,
        is_main: entry.is_main,
        dirty,
        locked: entry.locked,
        prunable: entry.prunable,
        outside_base,
        force: request.force,
        allow_outside_base: request.allow_outside_base,
        delete_remote: request.delete_remote,
        requires_remote_deleter: request.delete_remote,
        pr_state,
    })
}
