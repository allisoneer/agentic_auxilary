use git2::Repository;
use gwt_worktree::exec::remove::execute_remove_plan;
use gwt_worktree::plan::remove::RemoveRequest;
use gwt_worktree::plan::remove::plan_remove;
use gwt_worktree::pr::PullRequestLookup;
use gwt_worktree::pr::PullRequestState;
use gwt_worktree::remote::RemoteBranchDeleter;
use gwt_worktree::repo::ControlRepo;
use gwt_worktree::types::BranchName;
use std::error::Error;
use tempfile::TempDir;

#[test]
fn removes_clean_worktree() -> Result<(), Box<dyn Error>> {
    let fixture = make_worktree_fixture("feature")?;
    let plan = plan_remove(
        &fixture.control,
        &RemoveRequest {
            branch: BranchName::new("feature")?,
            force: false,
            allow_outside_base: false,
            delete_remote: false,
        },
        None,
    )?;

    execute_remove_plan(&fixture.control, &plan, None)?;
    assert!(!fixture.linked_path.exists());
    assert!(
        fixture
            .repo
            .find_branch("feature", git2::BranchType::Local)
            .is_err()
    );
    Ok(())
}

#[test]
fn dirty_worktree_requires_force() -> Result<(), Box<dyn Error>> {
    let fixture = make_worktree_fixture("dirty")?;
    std::fs::write(fixture.linked_path.join("dirty.txt"), "data")?;
    let plan = plan_remove(
        &fixture.control,
        &RemoveRequest {
            branch: BranchName::new("dirty")?,
            force: false,
            allow_outside_base: false,
            delete_remote: false,
        },
        None,
    )?;

    let error = execute_remove_plan(&fixture.control, &plan, None).expect_err("dirty should fail");
    assert!(error.to_string().contains("uncommitted changes"));
    Ok(())
}

#[test]
fn locked_worktree_requires_force() -> Result<(), Box<dyn Error>> {
    let fixture = make_worktree_fixture("locked")?;
    let linked_repo = Repository::open(&fixture.linked_path)?;
    git2::Worktree::open_from_repository(&linked_repo)?.lock(None)?;
    let plan = plan_remove(
        &fixture.control,
        &RemoveRequest {
            branch: BranchName::new("locked")?,
            force: false,
            allow_outside_base: false,
            delete_remote: false,
        },
        None,
    )?;

    let error = execute_remove_plan(&fixture.control, &plan, None).expect_err("locked should fail");
    assert!(error.to_string().contains("locked"));
    Ok(())
}

#[test]
fn outside_base_requires_override() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let outside_path = temp.path().join("elsewhere").join("feature");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;
    repo.branch("outside", &repo.head()?.peel_to_commit()?, false)?;
    std::fs::create_dir_all(outside_path.parent().ok_or("missing parent")?)?;
    let branch = repo.find_branch("outside", git2::BranchType::Local)?;
    let reference = branch.into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree("outside", &outside_path, Some(&options))?;
    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;

    let plan = plan_remove(
        &control,
        &RemoveRequest {
            branch: BranchName::new("outside")?,
            force: false,
            allow_outside_base: false,
            delete_remote: false,
        },
        None,
    )?;

    let error = execute_remove_plan(&control, &plan, None).expect_err("outside-base should fail");
    assert!(error.to_string().contains("outside"));
    Ok(())
}

#[test]
fn remote_delete_requires_deleter_and_pr_lookup_is_plugged() -> Result<(), Box<dyn Error>> {
    let fixture = make_worktree_fixture("remote")?;
    let plan = plan_remove(
        &fixture.control,
        &RemoveRequest {
            branch: BranchName::new("remote")?,
            force: true,
            allow_outside_base: false,
            delete_remote: true,
        },
        Some(&StubPrLookup),
    )?;

    assert_eq!(plan.pr_state, Some(PullRequestState::Merged));
    let error = execute_remove_plan(&fixture.control, &plan, None)
        .expect_err("remote delete should need deleter");
    assert!(error.to_string().contains("deleter"));

    execute_remove_plan(&fixture.control, &plan, Some(&StubRemoteDeleter))?;
    Ok(())
}

struct RemoveFixture {
    _temp: TempDir,
    repo: Repository,
    control: ControlRepo,
    linked_path: std::path::PathBuf,
}

struct StubPrLookup;

impl PullRequestLookup for StubPrLookup {
    fn lookup_pull_request_state(
        &self,
        _branch: &BranchName,
    ) -> gwt_worktree::Result<PullRequestState> {
        Ok(PullRequestState::Merged)
    }
}

struct StubRemoteDeleter;

impl RemoteBranchDeleter for StubRemoteDeleter {
    fn delete_remote_branch(
        &self,
        _repo: &Repository,
        _remote: &str,
        _branch: &BranchName,
    ) -> gwt_worktree::Result<()> {
        Ok(())
    }
}

fn make_worktree_fixture(branch_name: &str) -> Result<RemoveFixture, Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let linked_path = main_repo.with_extension("gwt").join(branch_name);
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;
    {
        let commit = repo.head()?.peel_to_commit()?;
        repo.branch(branch_name, &commit, false)?;
    }
    std::fs::create_dir_all(linked_path.parent().ok_or("missing parent")?)?;
    {
        let branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
        let reference = branch.into_reference();
        let mut options = git2::WorktreeAddOptions::new();
        options.reference(Some(&reference));
        repo.worktree(branch_name, &linked_path, Some(&options))?;
    }
    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;
    Ok(RemoveFixture {
        _temp: temp,
        repo,
        control,
        linked_path,
    })
}

fn commit_initial(repo: &Repository) -> Result<(), Box<dyn Error>> {
    let sig = git2::Signature::now("Test", "test@example.com")?;
    let tree_id = {
        let mut index = repo.index()?;
        index.write_tree()?
    };
    let tree = repo.find_tree(tree_id)?;
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])?;
    Ok(())
}
