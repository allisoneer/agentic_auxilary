pub mod freshness;

use anyhow::Context;
use anyhow::Result;
use git2::Repository;
use gwt_worktree::plan::switch::SwitchRequest;
use gwt_worktree::plan::switch::plan_switch;
use gwt_worktree::repo::ControlRepo;
use gwt_worktree::repo::ResolveControlRepoOptions;
use gwt_worktree::types::BranchName;
use gwt_worktree::worktree::list_worktrees;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TargetWorktree {
    pub path: PathBuf,
    pub branch: String,
    pub base_ref: String,
}

pub fn chdir_to(target: &TargetWorktree) -> Result<()> {
    std::env::set_current_dir(&target.path)
        .with_context(|| format!("failed to chdir into {}", target.path.display()))?;
    Ok(())
}

pub fn resolve(
    branch: Option<&str>,
    worktree: Option<&Path>,
    create_if_missing: bool,
) -> Result<TargetWorktree> {
    if let Some(path) = worktree {
        return resolve_from_path(path, branch);
    }

    if let Some(branch) = branch {
        return resolve_from_branch(branch, create_if_missing);
    }

    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    resolve_from_path(&cwd, None)
}

fn resolve_from_path(path: &Path, expected_branch: Option<&str>) -> Result<TargetWorktree> {
    let repo = Repository::discover(path)
        .with_context(|| format!("failed to discover git repository from {}", path.display()))?;
    let workdir = repo.workdir().map(Path::to_path_buf).ok_or_else(|| {
        anyhow::anyhow!("repository at {} has no working directory", path.display())
    })?;
    let branch = current_branch(&repo)?;

    if let Some(expected_branch) = expected_branch
        && branch != expected_branch
    {
        anyhow::bail!(
            "worktree branch mismatch: expected '{}', found '{}' at {}",
            expected_branch,
            branch,
            workdir.display()
        );
    }

    Ok(TargetWorktree {
        path: workdir,
        branch,
        base_ref: default_base_ref(&repo),
    })
}

fn resolve_from_branch(branch: &str, create_if_missing: bool) -> Result<TargetWorktree> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let control = ControlRepo::resolve(&ResolveControlRepoOptions {
        cwd: Some(&cwd),
        ..ResolveControlRepoOptions::default()
    })
    .context("failed to resolve gwt control repository")?;

    if let Some(existing) = list_worktrees(&control)?
        .into_iter()
        .find(|item| item.branch.as_deref() == Some(branch) && item.path.exists())
    {
        return Ok(TargetWorktree {
            path: existing.path,
            branch: branch.to_string(),
            base_ref: default_base_ref(&Repository::open(&control.common_dir)?),
        });
    }

    if !create_if_missing {
        anyhow::bail!("no worktree found for branch '{branch}'");
    }

    let branch_name = BranchName::new(branch.to_string())?;
    let plan = plan_switch(
        &control,
        &SwitchRequest {
            branch: branch_name.clone(),
            create: false,
            force_create: false,
            start_point: None,
            guess_remote: false,
        },
        None,
    )
    .or_else(|error| {
        if matches!(error, gwt_worktree::Error::BranchNotFound(_)) {
            plan_switch(
                &control,
                &SwitchRequest {
                    branch: branch_name,
                    create: true,
                    force_create: false,
                    start_point: None,
                    guess_remote: false,
                },
                None,
            )
        } else {
            Err(error)
        }
    })?;

    let execution = gwt_worktree::exec::switch::execute_switch_plan(&control, &plan, None)?;
    let repo = Repository::open(&control.common_dir)?;

    Ok(TargetWorktree {
        path: execution.path,
        branch: branch.to_string(),
        base_ref: default_base_ref(&repo),
    })
}

fn current_branch(repo: &Repository) -> Result<String> {
    let head = repo.head().context("failed to read HEAD")?;
    let branch = head
        .shorthand()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("repository is detached; branch is required"))?;
    Ok(branch)
}

fn default_base_ref(repo: &Repository) -> String {
    for candidate in ["origin/main", "origin/master"] {
        if repo
            .find_reference(&format!("refs/remotes/{candidate}"))
            .is_ok()
        {
            return candidate.to_string();
        }
    }

    "origin/main".to_string()
}
