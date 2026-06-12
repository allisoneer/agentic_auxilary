use git2::Repository;
use gwt_worktree::config::RepoConfig;
use gwt_worktree::exec::gc::execute_gc_plan;
use gwt_worktree::plan::gc::GcPolicy;
use gwt_worktree::plan::gc::plan_gc;
use gwt_worktree::repo::ControlRepo;
use std::error::Error;
use tempfile::TempDir;

#[test]
fn gc_plan_partitions_and_carries_clean_command() -> Result<(), Box<dyn Error>> {
    let fixture = make_gc_fixture()?;
    mark_repo_dirty(&fixture.dirty_path)?;

    let plan = plan_gc(
        &fixture.control,
        Some(&RepoConfig {
            clean_command: Some("just clean".into()),
            ..RepoConfig::default()
        }),
        &GcPolicy {
            clean_days: 0,
            delete_days: 0,
        },
    )?;

    assert_eq!(plan.commands_to_run.len(), 1);
    assert!(
        plan.to_delete
            .iter()
            .any(|item| item.path == fixture.merged_path)
    );
    assert!(
        plan.dirty
            .iter()
            .any(|item| item.path == fixture.dirty_path)
    );
    assert!(
        plan.unmerged
            .iter()
            .chain(plan.skip.iter())
            .chain(plan.dirty.iter())
            .any(|item| item.path == fixture.unmerged_path)
    );
    Ok(())
}

#[test]
fn gc_execution_deletes_only_authorized_entries() -> Result<(), Box<dyn Error>> {
    let fixture = make_gc_fixture()?;
    mark_repo_dirty(&fixture.dirty_path)?;
    let plan = plan_gc(
        &fixture.control,
        None,
        &GcPolicy {
            clean_days: 0,
            delete_days: 0,
        },
    )?;

    let result = execute_gc_plan(&fixture.control, &plan)?;

    assert!(result.deleted_paths.contains(&fixture.merged_path));
    assert!(!fixture.merged_path.exists());
    assert!(fixture.unmerged_path.exists());
    assert!(fixture.dirty_path.exists());
    Ok(())
}

fn mark_repo_dirty(path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let linked_repo = Repository::open(path)?;
    std::fs::write(path.join("dirty.txt"), "data")?;
    let mut index = linked_repo.index()?;
    index.add_path(std::path::Path::new("dirty.txt"))?;
    Ok(())
}

struct GcFixture {
    _temp: TempDir,
    control: ControlRepo,
    merged_path: std::path::PathBuf,
    dirty_path: std::path::PathBuf,
    unmerged_path: std::path::PathBuf,
}

fn make_gc_fixture() -> Result<GcFixture, Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;

    let merged_path = main_repo.with_extension("gwt").join("merged");
    let dirty_path = main_repo.with_extension("gwt").join("dirty");
    let unmerged_path = main_repo.with_extension("gwt").join("unmerged");
    create_branch_worktree(&repo, &merged_path, "merged", true)?;
    create_branch_worktree(&repo, &dirty_path, "dirty", true)?;
    create_branch_worktree(&repo, &unmerged_path, "unmerged", false)?;
    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;

    Ok(GcFixture {
        _temp: temp,
        control,
        merged_path,
        dirty_path,
        unmerged_path,
    })
}

fn create_branch_worktree(
    repo: &Repository,
    path: &std::path::Path,
    branch_name: &str,
    merge_to_main: bool,
) -> Result<(), Box<dyn Error>> {
    let commit = repo.head()?.peel_to_commit()?;
    repo.branch(branch_name, &commit, false)?;
    std::fs::create_dir_all(path.parent().ok_or("missing parent")?)?;
    let branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
    let reference = branch.into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree(branch_name, path, Some(&options))?;

    if !merge_to_main {
        let linked_repo = Repository::open(path)?;
        std::fs::write(path.join("new.txt"), branch_name)?;
        let sig = git2::Signature::now("Test", "test@example.com")?;
        let tree_id = {
            let mut index = linked_repo.index()?;
            index.add_path(std::path::Path::new("new.txt"))?;
            index.write_tree()?
        };
        let tree = linked_repo.find_tree(tree_id)?;
        let parent = linked_repo.head()?.peel_to_commit()?;
        linked_repo.commit(Some("HEAD"), &sig, &sig, "branch change", &tree, &[&parent])?;
    }
    Ok(())
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
