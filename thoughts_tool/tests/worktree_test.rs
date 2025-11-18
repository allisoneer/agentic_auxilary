use assert_cmd::cargo::cargo_bin_cmd;
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

    // Configure git for CI environment
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&main_repo)
        .output()?;
    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&main_repo)
        .output()?;

    // Run thoughts init in main repo
    cargo_bin_cmd!("thoughts")
        .current_dir(&main_repo)
        .arg("init")
        .assert()
        .success();

    // Create an initial commit (required for worktree)
    let output = std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "Initial commit"])
        .current_dir(&main_repo)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "Git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("Failed to create initial commit".into());
    }

    // Create worktree
    let output = std::process::Command::new("git")
        .args(["worktree", "add", worktree.to_str().unwrap(), "HEAD"])
        .current_dir(&main_repo)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "Git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("Failed to create worktree".into());
    }

    // Run thoughts init in worktree
    cargo_bin_cmd!("thoughts")
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

    // Configure git for CI environment
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&main_repo)
        .output()?;
    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&main_repo)
        .output()?;

    // Create an initial commit (required for worktree)
    let output = std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "Initial commit"])
        .current_dir(&main_repo)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "Git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("Failed to create initial commit".into());
    }

    // Create worktree
    let output = std::process::Command::new("git")
        .args(["worktree", "add", worktree.to_str().unwrap(), "HEAD"])
        .current_dir(&main_repo)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "Git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("Failed to create worktree".into());
    }

    // Try to init worktree without main initialized
    cargo_bin_cmd!("thoughts")
        .current_dir(&worktree)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Main repository must be initialized first",
        ));

    Ok(())
}

#[test]
fn test_worktree_config_routing() -> Result<(), Box<dyn std::error::Error>> {
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

    // Configure git for CI environment
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&main_repo)
        .output()?;
    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&main_repo)
        .output()?;

    // Run thoughts init in main repo
    cargo_bin_cmd!("thoughts")
        .current_dir(&main_repo)
        .arg("init")
        .assert()
        .success();

    // Create an initial commit (required for worktree)
    let output = std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "Initial commit"])
        .current_dir(&main_repo)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "Git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("Failed to create initial commit".into());
    }

    // Create worktree
    let output = std::process::Command::new("git")
        .args(["worktree", "add", worktree.to_str().unwrap(), "HEAD"])
        .current_dir(&main_repo)
        .output()?;
    if !output.status.success() {
        eprintln!(
            "Git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err("Failed to create worktree".into());
    }

    // Run thoughts init in worktree (this should NOT create a config in the worktree)
    cargo_bin_cmd!("thoughts")
        .current_dir(&worktree)
        .arg("init")
        .assert()
        .success();

    // Verify config exists in main repo (created by init in main)
    let main_config = main_repo.join(".thoughts").join("config.json");
    assert!(
        main_config.exists(),
        "Config should exist in main repo at {:?}",
        main_config
    );

    // Verify config does NOT exist in worktree
    let worktree_config = worktree.join(".thoughts").join("config.json");
    assert!(
        !worktree_config.exists(),
        "Config should NOT exist in worktree at {:?}",
        worktree_config
    );

    // Verify that config show from worktree also works
    cargo_bin_cmd!("thoughts")
        .current_dir(&worktree)
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Repository Configuration"));

    Ok(())
}
