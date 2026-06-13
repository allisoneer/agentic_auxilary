use crate::command::CommandSpec;
use crate::config::RepoConfig;
use crate::error::Result;
use crate::repo::ControlRepo;
use crate::worktree::is_worktree_dirty;
use crate::worktree::list_worktrees;
use git2::Repository;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcPolicy {
    pub clean_days: u64,
    pub delete_days: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcItem {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub age_days: u64,
    pub dirty: bool,
    pub merged_to_main: bool,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GcPlan {
    pub commands_to_run: Vec<CommandSpec>,
    pub to_clean: Vec<GcItem>,
    pub to_delete: Vec<GcItem>,
    pub dirty: Vec<GcItem>,
    pub unmerged: Vec<GcItem>,
    pub skip: Vec<GcItem>,
    pub prunable: Vec<GcItem>,
}

pub fn plan_gc(
    control_repo: &ControlRepo,
    repo_config: Option<&RepoConfig>,
    policy: &GcPolicy,
) -> Result<GcPlan> {
    let repo = Repository::open(&control_repo.common_dir)?;
    let main_oid = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .map(|commit| commit.id());
    let mut plan = GcPlan::default();
    if let Some(clean_command) = repo_config.and_then(|config| config.clean_command.clone()) {
        plan.commands_to_run.push(clean_command);
    }

    for entry in list_worktrees(control_repo)? {
        if entry.is_main {
            continue;
        }

        if entry.prunable || !entry.path.exists() {
            plan.prunable.push(GcItem {
                path: entry.path,
                branch: entry.branch,
                age_days: 0,
                dirty: false,
                merged_to_main: false,
                locked: entry.locked,
                prunable: true,
            });
            continue;
        }

        let metadata = std::fs::metadata(&entry.path)?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let age_days = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default()
            .as_secs()
            / 86_400;
        let linked_repo = Repository::open(&entry.path)?;
        let dirty = is_worktree_dirty(&linked_repo)?;
        let merged_to_main = match (
            main_oid,
            linked_repo
                .head()
                .ok()
                .and_then(|head| head.peel_to_commit().ok()),
        ) {
            (Some(main), Some(head_commit)) => {
                head_commit.id() == main
                    || repo
                        .graph_descendant_of(main, head_commit.id())
                        .unwrap_or(false)
            }
            _ => false,
        };
        let item = GcItem {
            path: entry.path,
            branch: entry.branch,
            age_days,
            dirty,
            merged_to_main,
            locked: entry.locked,
            prunable: entry.prunable,
        };

        if item.locked {
            plan.skip.push(item);
        } else if item.dirty {
            plan.dirty.push(item);
        } else if !item.merged_to_main && item.age_days >= policy.delete_days {
            plan.unmerged.push(item);
        } else if item.age_days >= policy.delete_days {
            plan.to_delete.push(item);
        } else if item.age_days >= policy.clean_days {
            plan.to_clean.push(item);
        } else {
            plan.skip.push(item);
        }
    }

    Ok(plan)
}
