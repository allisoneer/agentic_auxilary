use crate::error::Error;
use crate::error::Result;
use crate::pr::PullRequestLookup;
use crate::pr::PullRequestState;
use crate::remote::resolve_remote_for_branch_deletion;
use crate::repo::ControlRepo;
use crate::types::BranchName;
use crate::worktree::is_worktree_dirty;
use crate::worktree::list_worktrees;
use git2::Repository;
use std::path::Component;
use std::path::Path;
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
    pub remote_to_delete: Option<String>,
    pub pr_state: Option<PullRequestState>,
}

pub fn plan_remove(
    control_repo: &ControlRepo,
    request: &RemoveRequest,
    pr_lookup: Option<&dyn PullRequestLookup>,
) -> Result<RemovePlan> {
    let control = Repository::open(&control_repo.common_dir)?;
    let entries = list_worktrees(control_repo)?;
    let entry = entries
        .into_iter()
        .find(|entry| entry.branch.as_deref() == Some(request.branch.as_str()))
        .ok_or_else(|| Error::BranchNotFound(request.branch.to_string()))?;
    let dirty = if entry.is_main || entry.prunable || !entry.path.exists() {
        false
    } else {
        is_worktree_dirty(&Repository::open(&entry.path)?)?
    };
    let outside_base =
        !path_within_base_canonicalize_when_possible(&control_repo.worktree_base, &entry.path);
    let remote_to_delete = if request.delete_remote {
        Some(resolve_remote_for_branch_deletion(
            &control,
            &request.branch,
        )?)
    } else {
        None
    };
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
        remote_to_delete,
        pr_state,
    })
}

fn path_within_base_canonicalize_when_possible(base: &Path, path: &Path) -> bool {
    match (base.canonicalize(), path.canonicalize()) {
        (Ok(canonical_base), Ok(canonical_path)) => canonical_path.starts_with(canonical_base),
        _ => normalize_path(path).starts_with(normalize_path(base)),
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}
