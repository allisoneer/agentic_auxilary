use git2::Repository;
use gwt_worktree::repo::ControlRepo;
use gwt_worktree::worktree::is_worktree_dirty;
use gwt_worktree::worktree::list_worktrees;
use std::error::Error;
use tempfile::TempDir;

#[test]
fn listing_includes_main_and_linked_entries() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let temp_root = canonical_temp_root(&temp);
    let main_repo = temp_root.join("main");
    let linked_path = temp_root.join("main.gwt").join("feature");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;
    std::fs::create_dir_all(linked_path.parent().ok_or("missing parent")?)?;
    repo.worktree("feature", &linked_path, None)?;

    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8 path")?)?;
    let entries = list_worktrees(&control)?;

    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|entry| entry.is_main && entry.path == main_repo)
    );
    assert!(entries.iter().any(|entry| {
        !entry.is_main
            && entry.path == linked_path
            && entry.branch.as_deref() == Some("feature")
            && !entry.detached
    }));
    Ok(())
}

#[test]
fn dirty_helper_detects_untracked_files() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let repo = Repository::init(temp.path())?;
    commit_initial(&repo)?;

    std::fs::write(temp.path().join("untracked.txt"), "data")?;

    assert!(is_worktree_dirty(&repo)?);
    Ok(())
}

#[test]
fn listing_survives_missing_linked_worktree() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let temp_root = canonical_temp_root(&temp);
    let main_repo = temp_root.join("main");
    let linked_path = temp_root.join("main.gwt").join("feature");
    let repo = Repository::init(&main_repo)?;
    commit_initial(&repo)?;
    std::fs::create_dir_all(linked_path.parent().ok_or("missing parent")?)?;
    repo.worktree("feature", &linked_path, None)?;
    std::fs::remove_dir_all(&linked_path)?;

    let control = ControlRepo::from_git_dir(main_repo.to_str().ok_or("non-utf8 path")?)?;
    let entries = list_worktrees(&control)?;

    assert!(entries.iter().any(|entry| {
        !entry.is_main
            && entry.path == linked_path
            && entry.branch.as_deref() == Some("feature")
            && entry.prunable
    }));
    Ok(())
}

fn canonical_temp_root(temp: &TempDir) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        temp.path()
            .canonicalize()
            .expect("canonicalize TempDir path on macOS")
    }
    #[cfg(not(target_os = "macos"))]
    {
        temp.path().to_path_buf()
    }
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
