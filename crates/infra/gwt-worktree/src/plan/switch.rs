use crate::command::CommandSpec;
use crate::config::RepoConfig;
use crate::error::Error;
use crate::error::Result;
use crate::repo::ControlRepo;
use crate::types::AdminName;
use crate::types::BranchName;
use crate::worktree::list_worktrees;
use git2::BranchType;
use git2::Repository;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchStartPoint {
    Head,
    Commit(String),
    LocalBranch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchRequest {
    pub branch: BranchName,
    pub create: bool,
    pub force_create: bool,
    pub start_point: Option<SwitchStartPoint>,
    pub guess_remote: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchPlanKind {
    Main,
    ExistingWorktree,
    ExistingLocalBranch,
    CreateBranch { start_point: SwitchStartPoint },
    ForceCreateBranch { start_point: SwitchStartPoint },
    CreateTrackingBranch { refresh_required: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchPlan {
    pub branch: BranchName,
    pub admin_name: AdminName,
    pub target_path: PathBuf,
    pub kind: SwitchPlanKind,
    pub post_create_commands: Vec<CommandSpec>,
}

pub fn default_start_point() -> SwitchStartPoint {
    SwitchStartPoint::Head
}

pub fn plan_switch(
    control_repo: &ControlRepo,
    request: &SwitchRequest,
    repo_config: Option<&RepoConfig>,
) -> Result<SwitchPlan> {
    let repo = Repository::open(&control_repo.common_dir)?;
    let target_path = control_repo.worktree_base.join(request.branch.as_str());
    let admin_name = request.branch.encode_admin_name();
    let post_create_commands = repo_config
        .map(|config| config.post_create_commands.clone())
        .unwrap_or_default();

    let main_branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(ToOwned::to_owned));
    if main_branch.as_deref() == Some(request.branch.as_str())
        && control_repo.main_workdir.is_some()
    {
        return Ok(SwitchPlan {
            branch: request.branch.clone(),
            admin_name,
            target_path: control_repo.main_workdir.clone().unwrap_or(target_path),
            kind: SwitchPlanKind::Main,
            post_create_commands,
        });
    }

    if let Some(entry) = list_worktrees(control_repo)?.into_iter().find(|entry| {
        entry.branch.as_deref() == Some(request.branch.as_str())
            && !entry.prunable
            && entry.path.exists()
    }) {
        return Ok(SwitchPlan {
            branch: request.branch.clone(),
            admin_name,
            target_path: entry.path,
            kind: SwitchPlanKind::ExistingWorktree,
            post_create_commands,
        });
    }

    if repo
        .find_branch(request.branch.as_str(), BranchType::Local)
        .is_ok()
        && !request.create
        && !request.force_create
    {
        return Ok(SwitchPlan {
            branch: request.branch.clone(),
            admin_name,
            target_path,
            kind: SwitchPlanKind::ExistingLocalBranch,
            post_create_commands,
        });
    }

    if request.force_create {
        let Some(start_point) = request.start_point.clone() else {
            return Err(Error::MissingStartPoint);
        };
        return Ok(SwitchPlan {
            branch: request.branch.clone(),
            admin_name,
            target_path,
            kind: SwitchPlanKind::ForceCreateBranch { start_point },
            post_create_commands,
        });
    }

    if request.create {
        return Ok(SwitchPlan {
            branch: request.branch.clone(),
            admin_name,
            target_path,
            kind: SwitchPlanKind::CreateBranch {
                start_point: request
                    .start_point
                    .clone()
                    .unwrap_or_else(default_start_point),
            },
            post_create_commands,
        });
    }

    if request.guess_remote {
        return Ok(SwitchPlan {
            branch: request.branch.clone(),
            admin_name,
            target_path,
            kind: SwitchPlanKind::CreateTrackingBranch {
                refresh_required: true,
            },
            post_create_commands,
        });
    }

    Err(Error::BranchNotFound(request.branch.to_string()))
}
