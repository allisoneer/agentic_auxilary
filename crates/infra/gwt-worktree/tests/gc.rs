use filetime::FileTime;
use filetime::set_file_mtime;
use git2::Repository;
use gwt_worktree::config::RepoConfig;
use gwt_worktree::error::Error as GwtError;
use gwt_worktree::exec::gc::execute_gc_plan;
use gwt_worktree::plan::gc::GcItem;
use gwt_worktree::plan::gc::GcPlan;
use gwt_worktree::plan::gc::GcPolicy;
use gwt_worktree::plan::gc::plan_gc;
use gwt_worktree::repo::ControlRepo;
use std::error::Error;
use std::time::Duration;
use std::time::SystemTime;
use tempfile::TempDir;

#[test]
fn gc_plan_puts_unmerged_only_in_unmerged_bucket() -> Result<(), Box<dyn Error>> {
    let fixture = make_gc_fixture()?;

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
        plan.unmerged
            .iter()
            .any(|item| item.path == fixture.unmerged_path)
    );
    assert_not_in_bucket(&plan.to_delete, &fixture.unmerged_path);
    assert_not_in_bucket(&plan.to_clean, &fixture.unmerged_path);
    assert_not_in_bucket(&plan.skip, &fixture.unmerged_path);
    assert_not_in_bucket(&plan.dirty, &fixture.unmerged_path);
    assert_not_in_bucket(&plan.prunable, &fixture.unmerged_path);
    Ok(())
}

#[test]
fn gc_plan_partitions_to_clean_to_delete_skip_by_age_thresholds() -> Result<(), Box<dyn Error>> {
    let fixture = make_gc_fixture()?;

    let plan = plan_gc(
        &fixture.control,
        None,
        &GcPolicy {
            clean_days: 3,
            delete_days: 7,
        },
    )?;

    assert!(
        plan.to_delete
            .iter()
            .any(|item| item.path == fixture.merged_path)
    );
    assert!(
        plan.to_clean
            .iter()
            .any(|item| item.path == fixture.clean_path)
    );
    assert!(plan.skip.iter().any(|item| item.path == fixture.young_path));
    assert!(
        plan.dirty
            .iter()
            .any(|item| item.path == fixture.dirty_path)
    );
    Ok(())
}

#[test]
fn gc_plan_keeps_missing_prunable_entries_exclusive() -> Result<(), Box<dyn Error>> {
    let fixture = make_gc_fixture()?;
    std::fs::remove_dir_all(&fixture.prunable_path)?;

    let plan = plan_gc(
        &fixture.control,
        None,
        &GcPolicy {
            clean_days: 3,
            delete_days: 7,
        },
    )?;

    assert!(
        plan.prunable
            .iter()
            .any(|item| item.path == fixture.prunable_path && item.prunable)
    );
    assert_not_in_bucket(&plan.to_delete, &fixture.prunable_path);
    assert_not_in_bucket(&plan.to_clean, &fixture.prunable_path);
    assert_not_in_bucket(&plan.skip, &fixture.prunable_path);
    assert_not_in_bucket(&plan.dirty, &fixture.prunable_path);
    assert_not_in_bucket(&plan.unmerged, &fixture.prunable_path);
    Ok(())
}

#[test]
fn gc_plan_emits_broken_open_worktree_as_prunable_instead_of_erroring() -> Result<(), Box<dyn Error>>
{
    let fixture = make_gc_fixture()?;
    std::fs::remove_file(fixture.broken_open_path.join(".git"))?;

    let plan = plan_gc(
        &fixture.control,
        None,
        &GcPolicy {
            clean_days: 3,
            delete_days: 7,
        },
    )?;

    assert!(
        plan.prunable
            .iter()
            .any(|item| item.path == fixture.broken_open_path && item.prunable)
    );
    assert_not_in_bucket(&plan.to_delete, &fixture.broken_open_path);
    assert_not_in_bucket(&plan.to_clean, &fixture.broken_open_path);
    assert_not_in_bucket(&plan.skip, &fixture.broken_open_path);
    assert_not_in_bucket(&plan.dirty, &fixture.broken_open_path);
    assert_not_in_bucket(&plan.unmerged, &fixture.broken_open_path);
    Ok(())
}

#[test]
fn gc_execution_deletes_and_prunes_only_authorized_entries() -> Result<(), Box<dyn Error>> {
    let fixture = make_gc_fixture()?;
    std::fs::remove_dir_all(&fixture.prunable_path)?;
    let plan = plan_gc(
        &fixture.control,
        None,
        &GcPolicy {
            clean_days: 3,
            delete_days: 7,
        },
    )?;

    let result = execute_gc_plan(&fixture.control, &plan)?;

    assert!(result.deleted_paths.contains(&fixture.merged_path));
    assert!(result.pruned_paths.contains(&fixture.prunable_path));
    assert!(!fixture.merged_path.exists());
    assert!(fixture.clean_path.exists());
    assert!(fixture.young_path.exists());
    assert!(fixture.unmerged_path.exists());
    assert!(fixture.dirty_path.exists());
    Ok(())
}

#[test]
fn gc_execution_surfaces_prune_failures_for_unregistered_path() -> Result<(), Box<dyn Error>> {
    let fixture = make_gc_fixture()?;
    let unregistered_path = fixture.control.worktree_base.join("unregistered");
    let plan = GcPlan {
        prunable: vec![GcItem {
            path: unregistered_path.clone(),
            branch: Some(String::from("ghost")),
            age_days: 0,
            dirty: false,
            merged_to_main: false,
            locked: false,
            prunable: true,
        }],
        ..GcPlan::default()
    };

    match execute_gc_plan(&fixture.control, &plan) {
        Err(GwtError::RegisteredWorktreeNotFound(path)) => assert_eq!(path, unregistered_path),
        Err(other) => return Err(format!("expected registered-worktree error, got {other}").into()),
        Ok(result) => return Err(format!("expected failure, got {result:?}").into()),
    }

    Ok(())
}

fn assert_not_in_bucket(bucket: &[GcItem], path: &std::path::Path) {
    assert!(!bucket.iter().any(|item| item.path == path));
}

struct GcFixture {
    _temp: TempDir,
    control: ControlRepo,
    merged_path: std::path::PathBuf,
    clean_path: std::path::PathBuf,
    young_path: std::path::PathBuf,
    dirty_path: std::path::PathBuf,
    unmerged_path: std::path::PathBuf,
    prunable_path: std::path::PathBuf,
    broken_open_path: std::path::PathBuf,
}

fn make_gc_fixture() -> Result<GcFixture, Box<dyn Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;

    let merged_path = main_repo.with_extension("gwt").join("merged");
    let clean_path = main_repo.with_extension("gwt").join("clean");
    let young_path = main_repo.with_extension("gwt").join("young");
    let dirty_path = main_repo.with_extension("gwt").join("dirty");
    let unmerged_path = main_repo.with_extension("gwt").join("unmerged");
    let prunable_path = main_repo.with_extension("gwt").join("prunable");
    let broken_open_path = main_repo.with_extension("gwt").join("broken-open");

    create_branch_worktree(&repo, &merged_path, "merged", true)?;
    create_branch_worktree(&repo, &clean_path, "clean", true)?;
    create_branch_worktree(&repo, &young_path, "young", true)?;
    create_branch_worktree(&repo, &dirty_path, "dirty", true)?;
    create_branch_worktree(&repo, &unmerged_path, "unmerged", false)?;
    create_branch_worktree(&repo, &prunable_path, "prunable", true)?;
    create_branch_worktree(&repo, &broken_open_path, "broken-open", true)?;

    set_age_days(&merged_path, 10)?;
    set_age_days(&clean_path, 5)?;
    set_age_days(&young_path, 1)?;
    set_age_days(&dirty_path, 10)?;
    set_age_days(&unmerged_path, 10)?;
    set_age_days(&prunable_path, 10)?;
    set_age_days(&broken_open_path, 10)?;
    mark_repo_dirty(&dirty_path)?;

    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8")?)?;

    Ok(GcFixture {
        _temp: temp,
        control,
        merged_path,
        clean_path,
        young_path,
        dirty_path,
        unmerged_path,
        prunable_path,
        broken_open_path,
    })
}

fn set_age_days(path: &std::path::Path, age_days: u64) -> Result<(), Box<dyn Error>> {
    let seconds = age_days.saturating_mul(86_400);
    let timestamp = SystemTime::now() - Duration::from_secs(seconds);
    set_file_mtime(path, FileTime::from_system_time(timestamp))?;
    Ok(())
}

fn mark_repo_dirty(path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let linked_repo = Repository::open(path)?;
    std::fs::write(path.join("dirty.txt"), "data")?;
    let mut index = linked_repo.index()?;
    index.add_path(std::path::Path::new("dirty.txt"))?;
    Ok(())
}

fn create_branch_worktree(
    repo: &Repository,
    path: &std::path::Path,
    branch_name: &str,
    merge_to_main: bool,
) -> Result<(), Box<dyn Error>> {
    let commit = repo.head()?.peel_to_commit()?;
    repo.branch(branch_name, &commit, false)?;

    if !merge_to_main {
        let sig = git2::Signature::now("Test", "test@example.com")?;
        let tree = commit.tree()?;
        repo.commit(
            Some(&format!("refs/heads/{branch_name}")),
            &sig,
            &sig,
            "branch change",
            &tree,
            &[&commit],
        )?;
    }

    std::fs::create_dir_all(path.parent().ok_or("missing parent")?)?;
    let branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
    let reference = branch.into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree(branch_name, path, Some(&options))?;
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
