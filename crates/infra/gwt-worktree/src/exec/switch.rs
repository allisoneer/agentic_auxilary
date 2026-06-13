use crate::command::CommandSpec;
use crate::error::Error;
use crate::error::Result;
use crate::plan::switch::SwitchPlan;
use crate::plan::switch::SwitchPlanKind;
use crate::plan::switch::SwitchStartPoint;
use crate::remote::RemoteRefresher;
use crate::repo::ControlRepo;
use git2::BranchType;
use git2::Oid;
use git2::Repository;
use git2::WorktreeAddOptions;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchExecutionResult {
    pub path: PathBuf,
    pub post_create_commands: Vec<CommandSpec>,
}

pub fn execute_switch_plan(
    control_repo: &ControlRepo,
    plan: &SwitchPlan,
    remote_refresher: Option<&dyn RemoteRefresher>,
) -> Result<SwitchExecutionResult> {
    let repo = Repository::open(&control_repo.common_dir)?;
    control_repo.ensure_worktree_base()?;

    let path = match &plan.kind {
        SwitchPlanKind::Main | SwitchPlanKind::ExistingWorktree => {
            if !plan.target_path.exists() {
                return Err(Error::MissingWorktreePath(plan.target_path.clone()));
            }
            plan.target_path.clone()
        }
        SwitchPlanKind::ExistingLocalBranch => {
            attach_existing_branch(&repo, plan)?;
            plan.target_path.clone()
        }
        SwitchPlanKind::CreateBranch { start_point } => {
            create_branch_and_worktree(&repo, plan, start_point, false)?;
            plan.target_path.clone()
        }
        SwitchPlanKind::ForceCreateBranch { start_point } => {
            create_branch_and_worktree(&repo, plan, start_point, true)?;
            plan.target_path.clone()
        }
        SwitchPlanKind::CreateTrackingBranch { .. } => {
            create_tracking_branch_and_worktree(
                &repo,
                plan,
                remote_refresher.ok_or(Error::MissingRemoteRefresher)?,
            )?;
            plan.target_path.clone()
        }
    };

    Ok(SwitchExecutionResult {
        path,
        post_create_commands: plan.post_create_commands.clone(),
    })
}

fn attach_existing_branch(repo: &Repository, plan: &SwitchPlan) -> Result<()> {
    std::fs::create_dir_all(plan.target_path.parent().unwrap_or(&plan.target_path))?;
    let branch = repo.find_branch(plan.branch.as_str(), BranchType::Local)?;
    let reference = branch.into_reference();
    let mut options = WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree(plan.admin_name.as_str(), &plan.target_path, Some(&options))?;
    Ok(())
}

fn create_branch_and_worktree(
    repo: &Repository,
    plan: &SwitchPlan,
    start_point: &SwitchStartPoint,
    force: bool,
) -> Result<()> {
    std::fs::create_dir_all(plan.target_path.parent().unwrap_or(&plan.target_path))?;
    let commit = resolve_start_point(repo, start_point)?;
    repo.branch(plan.branch.as_str(), &commit, force)?;
    attach_existing_branch(repo, plan)
}

fn create_tracking_branch_and_worktree(
    repo: &Repository,
    plan: &SwitchPlan,
    remote_refresher: &dyn RemoteRefresher,
) -> Result<()> {
    remote_refresher.refresh(repo)?;
    let target = remote_refresher
        .resolve_branch_target(repo, &plan.branch)?
        .ok_or_else(|| Error::BranchNotFound(plan.branch.to_string()))?;
    let oid = Oid::from_str(&target.commit_oid)
        .map_err(|_| Error::InvalidObjectId(target.commit_oid.clone()))?;
    repo.reference(&target.refname, oid, true, "create remote-tracking ref")?;
    let commit = repo.find_commit(oid)?;
    let mut branch = repo.branch(plan.branch.as_str(), &commit, false)?;
    branch.set_upstream(Some(&format!("{}/{}", target.remote, plan.branch.as_str())))?;
    attach_existing_branch(repo, plan)
}

fn resolve_start_point<'a>(
    repo: &'a Repository,
    start_point: &SwitchStartPoint,
) -> Result<git2::Commit<'a>> {
    match start_point {
        SwitchStartPoint::Head => Ok(repo.head()?.peel_to_commit()?),
        SwitchStartPoint::Commit(oid) => {
            let oid = Oid::from_str(oid).map_err(|_| Error::InvalidObjectId(oid.clone()))?;
            Ok(repo.find_commit(oid)?)
        }
        SwitchStartPoint::LocalBranch(branch) => {
            let branch = repo.find_branch(branch, BranchType::Local)?;
            Ok(branch.get().peel_to_commit()?)
        }
    }
}
