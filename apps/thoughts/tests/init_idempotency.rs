mod support;

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::TempDir;

#[ignore = "integration test - run with: just test-integration"]
#[test]
fn init_twice_returns_ok() {
    let td = TempDir::new().unwrap();

    // Initialize git repo
    support::git_ok(td.path(), &["init"]);

    // First run
    cargo_bin_cmd!("thoughts")
        .current_dir(td.path())
        .arg("init")
        .assert()
        .success();

    // Second run should also succeed (idempotent)
    cargo_bin_cmd!("thoughts")
        .current_dir(td.path())
        .arg("init")
        .assert()
        .success();
}

#[ignore = "integration test - run with: just test-integration"]
#[test]
fn incorrect_symlink_requires_force() {
    let td = TempDir::new().unwrap();

    // Initialize git repo
    support::git_ok(td.path(), &["init"]);

    // First init creates correct symlinks
    cargo_bin_cmd!("thoughts")
        .current_dir(td.path())
        .arg("init")
        .assert()
        .success();

    // Corrupt one symlink target to point elsewhere
    #[cfg(unix)]
    {
        let wrong = td.path().join("thoughts");
        std::fs::remove_file(&wrong).unwrap();
        std::os::unix::fs::symlink("not-thoughts-data", &wrong).unwrap();
    }

    // Second init without --force should fail
    cargo_bin_cmd!("thoughts")
        .current_dir(td.path())
        .arg("init")
        .assert()
        .failure();
}

#[ignore = "integration test - run with: just test-integration"]
#[test]
fn gitignore_includes_backup_patterns() {
    let td = TempDir::new().unwrap();

    // Initialize git repo
    support::git_ok(td.path(), &["init"]);

    // Run init
    cargo_bin_cmd!("thoughts")
        .current_dir(td.path())
        .arg("init")
        .assert()
        .success();

    let gitignore = td.path().join(".gitignore");
    assert!(gitignore.exists());

    let content = fs::read_to_string(&gitignore).unwrap();
    assert!(content.contains("/.claude/settings.local.json.bak"));
    assert!(content.contains("/.claude/settings.local.json.malformed.*.bak"));
}
