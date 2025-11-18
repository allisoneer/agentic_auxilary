use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::TempDir;

#[test]
fn init_twice_returns_ok() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        return;
    }

    let td = TempDir::new().unwrap();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(td.path())
        .output()
        .unwrap();

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

#[test]
fn incorrect_symlink_requires_force() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        return;
    }

    let td = TempDir::new().unwrap();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(td.path())
        .output()
        .unwrap();

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

#[test]
fn gitignore_includes_backup_patterns() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        return;
    }

    let td = TempDir::new().unwrap();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(td.path())
        .output()
        .unwrap();

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
