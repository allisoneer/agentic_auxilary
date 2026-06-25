pub mod freshness;

use anyhow::Context;
use anyhow::Result;
use git2::Repository;
use gwt_worktree::plan::switch::SwitchPlan;
use gwt_worktree::plan::switch::SwitchPlanKind;
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

#[derive(Debug, Clone)]
pub struct WorktreePreview {
    pub path: Option<PathBuf>,
    pub branch: String,
    pub would_create: bool,
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

pub fn preview_resolve(branch: Option<&str>, worktree: Option<&Path>) -> Result<WorktreePreview> {
    if let Some(path) = worktree {
        let target = resolve_from_path(path, branch)?;
        return Ok(WorktreePreview {
            path: Some(target.path),
            branch: target.branch,
            would_create: false,
        });
    }

    if let Some(branch) = branch {
        return preview_from_branch(branch);
    }

    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let target = resolve_from_path(&cwd, None)?;
    Ok(WorktreePreview {
        path: Some(target.path),
        branch: target.branch,
        would_create: false,
    })
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

    let plan = plan_for_branch(&control, branch)?;
    let execution = gwt_worktree::exec::switch::execute_switch_plan(&control, &plan, None)?;
    let repo = Repository::open(&control.common_dir)?;

    Ok(TargetWorktree {
        path: execution.path,
        branch: branch.to_string(),
        base_ref: default_base_ref(&repo),
    })
}

fn preview_from_branch(branch: &str) -> Result<WorktreePreview> {
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
        return Ok(WorktreePreview {
            path: Some(existing.path),
            branch: branch.to_string(),
            would_create: false,
        });
    }

    let plan = plan_for_branch(&control, branch)?;
    Ok(WorktreePreview {
        path: Some(plan.target_path.clone()),
        branch: branch.to_string(),
        would_create: plan_would_create(&plan),
    })
}

fn plan_for_branch(control: &ControlRepo, branch: &str) -> Result<SwitchPlan> {
    let branch_name = BranchName::new(branch.to_string())?;
    plan_switch(
        control,
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
                control,
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
    })
    .map_err(Into::into)
}

fn plan_would_create(plan: &SwitchPlan) -> bool {
    !matches!(
        plan.kind,
        SwitchPlanKind::Main | SwitchPlanKind::ExistingWorktree
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use tempfile::TempDir;

    static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn preview_resolve_does_not_create_missing_branch_worktree() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.repo).unwrap();

        let before = worktree_paths(&fixture.repo).unwrap();
        let preview = preview_resolve(Some("feature/preview-only"), None).unwrap();
        let after = worktree_paths(&fixture.repo).unwrap();

        env::set_current_dir(saved).unwrap();
        assert_eq!(before, after);
        assert!(preview.would_create);
        assert!(preview.path.is_some());
        assert_eq!(preview.branch, "feature/preview-only");
    }

    fn cwd_lock() -> &'static Mutex<()> {
        CWD_LOCK.get_or_init(|| Mutex::new(()))
    }

    struct GitFixture {
        _temp: TempDir,
        repo: PathBuf,
    }

    impl GitFixture {
        fn new() -> Result<Self> {
            let temp = TempDir::new()?;
            let repo = temp.path().join("repo");

            run_git(temp.path(), ["init", repo.to_str().unwrap()])?;
            configure_repo(&repo)?;
            fs::write(repo.join("README.md"), "base\n")?;
            run_git(&repo, ["add", "README.md"])?;
            run_git(&repo, ["commit", "-m", "initial"])?;
            run_git(&repo, ["branch", "feature/preview-only"])?;

            Ok(Self { _temp: temp, repo })
        }
    }

    fn configure_repo(path: &Path) -> Result<()> {
        run_git(path, ["config", "user.name", "Test User"])?;
        run_git(path, ["config", "user.email", "test@example.com"])?;
        Ok(())
    }

    fn worktree_paths(cwd: &Path) -> Result<Vec<String>> {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(["worktree", "list", "--porcelain"])
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git worktree list failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("worktree ").map(str::to_string))
            .collect())
    }

    fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<()> {
        let output = Command::new("git").current_dir(cwd).args(args).output()?;
        if output.status.success() {
            Ok(())
        } else {
            anyhow::bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
    }

    use std::process::Command;
}
