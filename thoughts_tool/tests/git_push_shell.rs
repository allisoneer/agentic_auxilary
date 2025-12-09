//! Integration tests for shell-based git push operations.
//! These tests verify that push_current_branch works correctly via system git.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_push_shell

use std::fs;
use std::process::Command;
use tempfile::TempDir;

use thoughts_tool::git::shell_push::push_current_branch;

#[test]
fn push_to_bare_remote_with_shell_git() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create bare remote
    let remote = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--bare"])
            .arg(remote.path())
            .status()
            .unwrap()
            .success()
    );

    // Create local repo with content
    let local = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(local.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(local.path().join("c.txt"), "hello").unwrap();
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init"
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["branch", "-M", "main"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );

    // Push using our shell-based push
    push_current_branch(local.path(), "origin", "main").expect("push should succeed");

    // Verify remote has the ref
    let out = Command::new("git")
        .args([
            "ls-remote",
            remote.path().to_str().unwrap(),
            "refs/heads/main",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("refs/heads/main"));
}

#[test]
fn push_additional_commit() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create bare remote
    let remote = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--bare"])
            .arg(remote.path())
            .status()
            .unwrap()
            .success()
    );

    // Create local repo
    let local = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(local.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(local.path().join("a.txt"), "first").unwrap();
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "first"
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["branch", "-M", "main"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );

    // First push
    push_current_branch(local.path(), "origin", "main").expect("first push should succeed");

    // Get first commit SHA
    let out1 = Command::new("git")
        .current_dir(local.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let first_sha = String::from_utf8_lossy(&out1.stdout).trim().to_string();

    // Add another commit
    fs::write(local.path().join("b.txt"), "second").unwrap();
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "second"
            ])
            .status()
            .unwrap()
            .success()
    );

    // Get second commit SHA
    let out2 = Command::new("git")
        .current_dir(local.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let second_sha = String::from_utf8_lossy(&out2.stdout).trim().to_string();

    // Second push
    push_current_branch(local.path(), "origin", "main").expect("second push should succeed");

    // Verify remote has the new commit
    let out = Command::new("git")
        .args([
            "ls-remote",
            remote.path().to_str().unwrap(),
            "refs/heads/main",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&second_sha));
    assert!(!stdout.contains(&first_sha)); // First SHA no longer at HEAD
}

#[test]
fn push_nothing_to_push_succeeds() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create bare remote
    let remote = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--bare"])
            .arg(remote.path())
            .status()
            .unwrap()
            .success()
    );

    // Create local repo and push
    let local = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(local.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(local.path().join("a.txt"), "content").unwrap();
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init"
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["branch", "-M", "main"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(local.path())
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );

    // First push
    push_current_branch(local.path(), "origin", "main").expect("first push should succeed");

    // Push again without changes - should succeed with "everything up-to-date"
    push_current_branch(local.path(), "origin", "main")
        .expect("push with nothing to push should succeed");
}
