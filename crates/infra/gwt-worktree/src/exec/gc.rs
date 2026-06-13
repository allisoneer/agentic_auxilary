use crate::command::CommandSpec;
use crate::error::Error;
use crate::error::Result;
use crate::plan::gc::GcPlan;
use crate::repo::ControlRepo;
use crate::worktree::find_worktree_by_path;
use git2::BranchType;
use git2::Repository;
use git2::WorktreePruneOptions;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcExecutionResult {
    pub commands_to_run: Vec<CommandSpec>,
    pub deleted_paths: Vec<std::path::PathBuf>,
    pub pruned_paths: Vec<std::path::PathBuf>,
}

pub fn execute_gc_plan(control_repo: &ControlRepo, plan: &GcPlan) -> Result<GcExecutionResult> {
    let control = Repository::open(&control_repo.common_dir)?;
    let mut deleted_paths = Vec::new();
    let mut pruned_paths = Vec::new();

    for item in &plan.prunable {
        prune_at_path(&control, &item.path, item.locked)?;
        pruned_paths.push(item.path.clone());
    }

    for item in &plan.to_delete {
        prune_at_path(&control, &item.path, false)?;
        if let Some(branch) = &item.branch
            && let Ok(mut local_branch) = control.find_branch(branch, BranchType::Local)
        {
            local_branch.delete()?;
        }
        deleted_paths.push(item.path.clone());
    }

    Ok(GcExecutionResult {
        commands_to_run: plan.commands_to_run.clone(),
        deleted_paths,
        pruned_paths,
    })
}

fn prune_at_path(control: &Repository, path: &Path, locked: bool) -> Result<()> {
    let worktree = find_worktree_by_path(control, path)?
        .ok_or_else(|| Error::RegisteredWorktreeNotFound(path.to_path_buf()))?;

    let mut prune_options = WorktreePruneOptions::new();
    prune_options.valid(true).working_tree(true).locked(locked);
    worktree.prune(Some(&mut prune_options))?;
    Ok(())
}
