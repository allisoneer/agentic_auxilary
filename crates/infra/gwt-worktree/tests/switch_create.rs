use git2::BranchType;
use git2::Repository;
use gwt_worktree::config::RepoConfig;
use gwt_worktree::exec::switch::execute_switch_plan;
use gwt_worktree::plan::switch::SwitchPlanKind;
use gwt_worktree::plan::switch::SwitchRequest;
use gwt_worktree::plan::switch::plan_switch;
use gwt_worktree::remote::RemoteBranchTarget;
use gwt_worktree::remote::RemoteRefresher;
use gwt_worktree::repo::ControlRepo;
use gwt_worktree::types::BranchName;
use std::error::Error;
use tempfile::TempDir;

#[test]
fn reuses_existing_worktree() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let linked_path = temp.path().join("main.gwt").join("feature");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;
    std::fs::create_dir_all(linked_path.parent().ok_or("missing parent")?)?;
    repo.worktree("feature", &linked_path, None)?;
    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;

    let plan = plan_switch(
        &control,
        &SwitchRequest {
            branch: BranchName::new("feature")?,
            create: false,
            force_create: false,
            start_point: None,
            guess_remote: false,
        },
        None,
    )?;

    assert!(matches!(plan.kind, SwitchPlanKind::ExistingWorktree));
    let result = execute_switch_plan(&control, &plan, None)?;
    assert_eq!(result.path, linked_path);
    Ok(())
}

#[test]
fn creates_new_branch_with_post_create_commands_as_data_only() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;
    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;
    let repo_config = RepoConfig {
        post_create_commands: vec!["thoughts init".into()],
        ..RepoConfig::default()
    };

    let plan = plan_switch(
        &control,
        &SwitchRequest {
            branch: BranchName::new("feature/create")?,
            create: true,
            force_create: false,
            start_point: None,
            guess_remote: false,
        },
        Some(&repo_config),
    )?;

    assert!(matches!(plan.kind, SwitchPlanKind::CreateBranch { .. }));
    let result = execute_switch_plan(&control, &plan, None)?;
    assert_eq!(
        result.post_create_commands,
        repo_config.post_create_commands
    );
    assert!(result.path.exists());
    Ok(())
}

#[test]
fn force_create_requires_explicit_start_point() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;
    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;

    let Err(error) = plan_switch(
        &control,
        &SwitchRequest {
            branch: BranchName::new("feature/force")?,
            create: false,
            force_create: true,
            start_point: None,
            guess_remote: false,
        },
        None,
    ) else {
        return Err("force-create should require a start point".into());
    };

    assert!(error.to_string().contains("missing switch start point"));
    Ok(())
}

#[test]
fn remote_guess_requires_provider_and_sets_upstream() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let repo = Repository::init(&main_repo)?;
    let commit_oid = commit_initial(&repo)?;
    repo.remote("origin", "https://example.com/origin.git")?;
    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;

    let plan = plan_switch(
        &control,
        &SwitchRequest {
            branch: BranchName::new("feature/remote")?,
            create: false,
            force_create: false,
            start_point: None,
            guess_remote: true,
        },
        None,
    )?;

    let Err(missing_provider) = execute_switch_plan(&control, &plan, None) else {
        return Err("missing provider should fail".into());
    };
    assert!(
        missing_provider
            .to_string()
            .contains("remote refresh capability")
    );

    let provider = StubRemoteRefresher {
        target: Some(RemoteBranchTarget {
            remote: "origin".to_string(),
            refname: "refs/remotes/origin/feature/remote".to_string(),
            commit_oid,
        }),
    };
    execute_switch_plan(&control, &plan, Some(&provider))?;

    let branch = repo.find_branch("feature/remote", BranchType::Local)?;
    let upstream = branch.upstream()?;
    assert_eq!(upstream.name()?, Some("origin/feature/remote"));
    Ok(())
}

struct StubRemoteRefresher {
    target: Option<RemoteBranchTarget>,
}

impl RemoteRefresher for StubRemoteRefresher {
    fn refresh(&self, _repo: &Repository) -> gwt_worktree::Result<()> {
        Ok(())
    }

    fn resolve_branch_target(
        &self,
        _repo: &Repository,
        _branch: &BranchName,
    ) -> gwt_worktree::Result<Option<RemoteBranchTarget>> {
        Ok(self.target.clone())
    }
}

fn commit_initial(repo: &Repository) -> Result<String, Box<dyn Error>> {
    let sig = git2::Signature::now("Test", "test@example.com")?;
    let tree_id = {
        let mut index = repo.index()?;
        index.write_tree()?
    };
    let tree = repo.find_tree(tree_id)?;
    let oid = repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])?;
    Ok(oid.to_string())
}
