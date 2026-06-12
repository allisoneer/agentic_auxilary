use crate::error::Result;
use crate::repo::ControlRepo;
use git2::ErrorCode;
use git2::Repository;
use git2::StatusOptions;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub is_main: bool,
    pub locked: bool,
    pub prunable: bool,
    pub detached: bool,
}

pub fn list_worktrees(control_repo: &ControlRepo) -> Result<Vec<WorktreeInfo>> {
    let repo = Repository::open(&control_repo.common_dir)?;
    let mut items = Vec::new();

    if let Some(main_workdir) = &control_repo.main_workdir {
        items.push(worktree_info_from_repo(
            main_workdir.clone(),
            &repo,
            true,
            false,
            false,
        )?);
    }

    let worktrees = repo.worktrees()?;
    for name in (&worktrees).into_iter().flatten() {
        let worktree = repo.find_worktree(name)?;
        let linked_repo = Repository::open_from_worktree(&worktree)?;
        let locked = !matches!(worktree.is_locked()?, git2::WorktreeLockStatus::Unlocked);
        let prunable = worktree.is_prunable(None)?;
        items.push(worktree_info_from_repo(
            worktree.path().to_path_buf(),
            &linked_repo,
            false,
            locked,
            prunable,
        )?);
    }

    Ok(items)
}

pub fn is_worktree_dirty(repo: &Repository) -> Result<bool> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    Ok(!statuses.is_empty())
}

fn worktree_info_from_repo(
    path: PathBuf,
    repo: &Repository,
    is_main: bool,
    locked: bool,
    prunable: bool,
) -> Result<WorktreeInfo> {
    let (head, branch, detached) = inspect_head(repo)?;
    Ok(WorktreeInfo {
        path,
        head,
        branch,
        is_main,
        locked,
        prunable,
        detached,
    })
}

fn inspect_head(repo: &Repository) -> Result<(Option<String>, Option<String>, bool)> {
    match repo.head() {
        Ok(head) => {
            let branch = head.shorthand().map(ToOwned::to_owned);
            let detached = !head.is_branch();
            let head = head
                .peel_to_commit()
                .ok()
                .map(|commit| commit.id().to_string().chars().take(10).collect());
            Ok((head, branch, detached))
        }
        Err(error) if error.code() == ErrorCode::UnbornBranch => {
            let branch = repo
                .find_reference("HEAD")?
                .symbolic_target()
                .map(|name| name.trim_start_matches("refs/heads/").to_owned());
            Ok((None, branch, false))
        }
        Err(error) => Err(error.into()),
    }
}
