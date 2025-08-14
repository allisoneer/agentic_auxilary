use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_worktree_initialization() -> Result<(), Box<dyn std::error::Error>> {
    // Create temp directory for test
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let worktree = temp.path().join("worktree");

    // Initialize main repository
    fs::create_dir(&main_repo)?;
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&main_repo)
        .output()?;

    // Run thoughts init in main repo
    Command::cargo_bin("thoughts")?
        .current_dir(&main_repo)
        .arg("init")
        .assert()
        .success();

    // Create an initial commit (required for worktree)
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "Initial commit"])
        .current_dir(&main_repo)
        .output()?;

    // Create worktree
    std::process::Command::new("git")
        .args(["worktree", "add", worktree.to_str().unwrap(), "HEAD"])
        .current_dir(&main_repo)
        .output()?;

    // Run thoughts init in worktree
    Command::cargo_bin("thoughts")?
        .current_dir(&worktree)
        .arg("init")
        .assert()
        .success()
        .stderr(predicate::str::contains("Detected git worktree"));

    // Verify symlink structure
    let worktree_thoughts_data = worktree.join(".thoughts-data");
    assert!(worktree_thoughts_data.is_symlink());

    // Verify points to main repo
    let target = fs::read_link(&worktree_thoughts_data)?;
    assert!(target.to_string_lossy().contains("main"));

    Ok(())
}

#[test]
fn test_worktree_requires_main_init() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let main_repo = temp.path().join("main");
    let worktree = temp.path().join("worktree");

    // Create uninitialized main repo
    fs::create_dir(&main_repo)?;
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&main_repo)
        .output()?;

    // Create an initial commit (required for worktree)
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "Initial commit"])
        .current_dir(&main_repo)
        .output()?;

    // Create worktree
    std::process::Command::new("git")
        .args(["worktree", "add", worktree.to_str().unwrap(), "HEAD"])
        .current_dir(&main_repo)
        .output()?;

    // Try to init worktree without main initialized
    Command::cargo_bin("thoughts")?
        .current_dir(&worktree)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Main repository must be initialized first",
        ));

    Ok(())
}
