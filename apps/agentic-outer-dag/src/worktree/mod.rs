pub mod freshness;

use anyhow::Context;
use anyhow::Result;
use git2::Repository;
use gwt_worktree::command::CommandSpec;
use gwt_worktree::config::GwtConfig;
use gwt_worktree::config::RepoConfig;
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
use std::process::Command;

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

    let repo_config = load_repo_config(&control);
    let plan = plan_for_branch(&control, branch, repo_config.as_ref())?;
    let would_create = plan_would_create(&plan);
    let execution = gwt_worktree::exec::switch::execute_switch_plan(&control, &plan, None)?;
    if would_create {
        run_post_create_commands(&execution.path, &execution.post_create_commands);
    }
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

    let repo_config = load_repo_config(&control);
    let plan = plan_for_branch(&control, branch, repo_config.as_ref())?;
    Ok(WorktreePreview {
        path: Some(plan.target_path.clone()),
        branch: branch.to_string(),
        would_create: plan_would_create(&plan),
    })
}

fn plan_for_branch(
    control: &ControlRepo,
    branch: &str,
    repo_config: Option<&RepoConfig>,
) -> Result<SwitchPlan> {
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
        repo_config,
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
                repo_config,
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

fn load_repo_config(control: &ControlRepo) -> Option<RepoConfig> {
    match GwtConfig::load() {
        Ok(config) => config.repos.get(&control.git_dir_key).cloned(),
        Err(error) => {
            tracing::warn!(
                error = %error,
                git_dir_key = %control.git_dir_key,
                "failed to load gwt config; continuing without post-create commands"
            );
            None
        }
    }
}

fn run_post_create_commands(worktree_path: &Path, commands: &[CommandSpec]) {
    for command in commands {
        match Command::new("sh")
            .arg("-c")
            .arg(command.as_str())
            .current_dir(worktree_path)
            .output()
        {
            Ok(output) if output.status.success() => {
                tracing::info!(
                    command = %command,
                    cwd = %worktree_path.display(),
                    "completed gwt post-create command"
                );
            }
            Ok(output) => {
                tracing::warn!(
                    command = %command,
                    cwd = %worktree_path.display(),
                    status = ?output.status.code(),
                    stdout = %String::from_utf8_lossy(&output.stdout).trim(),
                    stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                    "gwt post-create command failed; continuing"
                );
            }
            Err(error) => {
                tracing::warn!(
                    command = %command,
                    cwd = %worktree_path.display(),
                    error = %error,
                    "failed to start gwt post-create command; continuing"
                );
            }
        }
    }
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
    use std::ffi::OsString;
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

    #[test]
    fn resolve_runs_post_create_commands_for_new_worktree() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        fixture.branch("feature/post-create").unwrap();
        let _config_guard = ConfigHomeGuard::new().unwrap();
        let config_path =
            write_gwt_config(&fixture.repo_git_dir(), &["echo ready > created.txt"]).unwrap();
        assert_gwt_config_loads_repo(&config_path, &fixture.repo_git_dir());
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.repo).unwrap();

        let target = resolve(Some("feature/post-create"), None, true).unwrap();

        env::set_current_dir(saved).unwrap();
        assert_eq!(
            fs::read_to_string(target.path.join("created.txt")).unwrap(),
            "ready\n"
        );
    }

    #[test]
    fn resolve_does_not_rerun_post_create_commands_for_existing_worktree() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        fixture.branch("feature/no-rerun").unwrap();
        let _config_guard = ConfigHomeGuard::new().unwrap();
        let config_path =
            write_gwt_config(&fixture.repo_git_dir(), &["echo run >> run-count.txt"]).unwrap();
        assert_gwt_config_loads_repo(&config_path, &fixture.repo_git_dir());
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.repo).unwrap();

        let first = resolve(Some("feature/no-rerun"), None, true).unwrap();
        let second = resolve(Some("feature/no-rerun"), None, true).unwrap();

        env::set_current_dir(saved).unwrap();
        assert_eq!(first.path, second.path);
        assert_eq!(
            fs::read_to_string(first.path.join("run-count.txt")).unwrap(),
            "run\n"
        );
    }

    #[test]
    fn resolve_runs_post_create_commands_from_worktree_cwd() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        fixture.branch("feature/cwd-check").unwrap();
        let _config_guard = ConfigHomeGuard::new().unwrap();
        let config_path = write_gwt_config(&fixture.repo_git_dir(), &["pwd > cwd.txt"]).unwrap();
        assert_gwt_config_loads_repo(&config_path, &fixture.repo_git_dir());
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.repo).unwrap();

        let target = resolve(Some("feature/cwd-check"), None, true).unwrap();

        env::set_current_dir(saved).unwrap();
        assert_eq!(
            fs::read_to_string(target.path.join("cwd.txt"))
                .unwrap()
                .trim(),
            target.path.display().to_string()
        );
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

        fn branch(&self, name: &str) -> Result<()> {
            run_git(&self.repo, ["branch", name])
        }

        fn repo_git_dir(&self) -> String {
            self.repo.join(".git").display().to_string()
        }
    }

    struct ConfigHomeGuard {
        _temp: TempDir,
        previous_xdg_config_home: Option<OsString>,
        previous_home: Option<OsString>,
    }

    impl ConfigHomeGuard {
        fn new() -> Result<Self> {
            let temp = TempDir::new()?;
            let xdg_config_home = temp.path().join("xdg-config");
            let home = temp.path().join("home");
            fs::create_dir_all(&xdg_config_home)?;
            fs::create_dir_all(&home)?;
            let previous_xdg_config_home = env::var_os("XDG_CONFIG_HOME");
            let previous_home = env::var_os("HOME");
            // SAFETY: tests serialize cwd/env mutation with a global mutex and restore it on drop.
            unsafe {
                env::set_var("XDG_CONFIG_HOME", &xdg_config_home);
                env::set_var("HOME", &home);
            }
            Ok(Self {
                _temp: temp,
                previous_xdg_config_home,
                previous_home,
            })
        }
    }

    impl Drop for ConfigHomeGuard {
        fn drop(&mut self) {
            match &self.previous_xdg_config_home {
                Some(previous) => {
                    // SAFETY: tests serialize cwd/env mutation with a global mutex and restore it on drop.
                    unsafe {
                        env::set_var("XDG_CONFIG_HOME", previous);
                    }
                }
                None => {
                    // SAFETY: tests serialize cwd/env mutation with a global mutex and restore it on drop.
                    unsafe {
                        env::remove_var("XDG_CONFIG_HOME");
                    }
                }
            }

            match &self.previous_home {
                Some(previous) => {
                    // SAFETY: tests serialize cwd/env mutation with a global mutex and restore it on drop.
                    unsafe {
                        env::set_var("HOME", previous);
                    }
                }
                None => {
                    // SAFETY: tests serialize cwd/env mutation with a global mutex and restore it on drop.
                    unsafe {
                        env::remove_var("HOME");
                    }
                }
            }
        }
    }

    fn configure_repo(path: &Path) -> Result<()> {
        run_git(path, ["config", "user.name", "Test User"])?;
        run_git(path, ["config", "user.email", "test@example.com"])?;
        Ok(())
    }

    fn write_gwt_config(repo_git_dir: &str, commands: &[&str]) -> Result<PathBuf> {
        let config_path = gwt_worktree::config::config_path()?;
        let Some(gwt_dir) = config_path.parent() else {
            anyhow::bail!("gwt config path has no parent directory");
        };
        fs::create_dir_all(gwt_dir)?;
        let serialized_commands = commands
            .iter()
            .map(|command| format!("    {command:?}"))
            .collect::<Vec<_>>()
            .join(",\n");
        fs::write(
            &config_path,
            format!(
                "[repos.{repo_git_dir:?}]\npost_create_commands = [\n{serialized_commands}\n]\n"
            ),
        )?;
        Ok(config_path)
    }

    fn assert_gwt_config_loads_repo(config_path: &Path, repo_git_dir: &str) {
        assert!(config_path.exists());
        let loaded = GwtConfig::load().unwrap();
        assert!(loaded.repos.contains_key(repo_git_dir));
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
}
