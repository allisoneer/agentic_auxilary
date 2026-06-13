use crate::error::Error;
use crate::error::Result;
use crate::plan::remove::RemovePlan;
use crate::remote::RemoteBranchDeleter;
use crate::repo::ControlRepo;
use crate::worktree::find_worktree_by_path;
use crate::worktree::is_worktree_dirty;
use git2::BranchType;
use git2::Repository;
use git2::WorktreePruneOptions;

pub fn execute_remove_plan(
    control_repo: &ControlRepo,
    plan: &RemovePlan,
    remote_deleter: Option<&dyn RemoteBranchDeleter>,
) -> Result<()> {
    if plan.is_main {
        return Err(Error::CannotRemoveMainWorktree);
    }
    if plan.outside_base && !plan.allow_outside_base {
        return Err(Error::WorktreeOutsideBase);
    }

    if !plan.prunable && plan.worktree_path.exists() {
        let linked_repo = Repository::open(&plan.worktree_path)?;
        if is_worktree_dirty(&linked_repo)? && !plan.force {
            return Err(Error::DirtyWorktree);
        }
    }

    let control = Repository::open(&control_repo.common_dir)?;
    let worktree = find_worktree_by_path(&control, &plan.worktree_path)?
        .ok_or_else(|| Error::RegisteredWorktreeNotFound(plan.worktree_path.clone()))?;
    let locked = !matches!(worktree.is_locked()?, git2::WorktreeLockStatus::Unlocked);
    if locked && !plan.force {
        return Err(Error::LockedWorktree);
    }

    let deleter = if plan.delete_remote {
        Some(remote_deleter.ok_or(Error::MissingRemoteBranchDeleter)?)
    } else {
        None
    };

    let mut prune_options = WorktreePruneOptions::new();
    prune_options
        .valid(true)
        .working_tree(true)
        .locked(plan.force || locked);
    worktree.prune(Some(&mut prune_options))?;

    if let Ok(mut branch) = control.find_branch(plan.branch.as_str(), BranchType::Local) {
        branch.delete()?;
    }

    if let Some(deleter) = deleter {
        deleter.delete_remote_branch(&control, "origin", &plan.branch)?;
    }

    Ok(())
}
