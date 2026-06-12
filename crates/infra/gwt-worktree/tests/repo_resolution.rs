use git2::Repository;
use gwt_worktree::config::GwtConfig;
use gwt_worktree::repo::ControlRepo;
use gwt_worktree::repo::ResolveControlRepoOptions;
use std::error::Error;
use tempfile::TempDir;

#[test]
fn resolves_from_cwd_discovery() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    let repo = Repository::init(&repo_root)?;
    commit_initial(&repo)?;
    let nested = repo_root.join("src").join("nested");
    std::fs::create_dir_all(&nested)?;

    let resolved = ControlRepo::resolve(&ResolveControlRepoOptions {
        cwd: Some(nested.as_path()),
        ..ResolveControlRepoOptions::default()
    })?;

    assert_eq!(
        resolved.git_dir_key,
        repo_root.join(".git").to_string_lossy()
    );
    assert_eq!(resolved.main_workdir, Some(repo_root));
    Ok(())
}

#[test]
fn resolves_from_config_when_other_sources_missing() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new()?;
    let repo_root = temp.path().join("repo");
    Repository::init(&repo_root)?;
    let config = GwtConfig {
        default_repo: Some(repo_root.join(".git").to_string_lossy().into_owned()),
        ..GwtConfig::default()
    };

    let resolved = ControlRepo::resolve(&ResolveControlRepoOptions {
        cwd: Some(temp.path()),
        config: Some(&config),
        ..ResolveControlRepoOptions::default()
    })?;

    assert_eq!(
        resolved.git_dir_key,
        repo_root.join(".git").to_string_lossy()
    );
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
