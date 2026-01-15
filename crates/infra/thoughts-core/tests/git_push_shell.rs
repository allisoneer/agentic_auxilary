//! Integration tests for shell-based git push operations.
//! These tests verify that push_current_branch works correctly via system git.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_push_shell

mod support;

use std::fs;
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
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create local repo with content
    let local = TempDir::new().unwrap();
    support::git_ok(local.path(), &["init"]);
    fs::write(local.path().join("c.txt"), "hello").unwrap();
    support::git_ok(local.path(), &["add", "."]);
    support::git_ok(
        local.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ],
    );
    support::git_ok(local.path(), &["branch", "-M", "main"]);
    support::git_ok(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );

    // Push using our shell-based push
    push_current_branch(local.path(), "origin", "main").expect("push should succeed");

    // Verify remote has the ref
    let stdout = support::git_stdout(remote.path(), &["ls-remote", ".", "refs/heads/main"]);
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
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create local repo
    let local = TempDir::new().unwrap();
    support::git_ok(local.path(), &["init"]);
    fs::write(local.path().join("a.txt"), "first").unwrap();
    support::git_ok(local.path(), &["add", "."]);
    support::git_ok(
        local.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "first",
        ],
    );
    support::git_ok(local.path(), &["branch", "-M", "main"]);
    support::git_ok(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );

    // First push
    push_current_branch(local.path(), "origin", "main").expect("first push should succeed");

    // Get first commit SHA
    let first_sha = support::git_stdout(local.path(), &["rev-parse", "HEAD"]);

    // Add another commit
    fs::write(local.path().join("b.txt"), "second").unwrap();
    support::git_ok(local.path(), &["add", "."]);
    support::git_ok(
        local.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "second",
        ],
    );

    // Get second commit SHA
    let second_sha = support::git_stdout(local.path(), &["rev-parse", "HEAD"]);

    // Second push
    push_current_branch(local.path(), "origin", "main").expect("second push should succeed");

    // Verify remote has the new commit
    let stdout = support::git_stdout(remote.path(), &["ls-remote", ".", "refs/heads/main"]);
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
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create local repo and push
    let local = TempDir::new().unwrap();
    support::git_ok(local.path(), &["init"]);
    fs::write(local.path().join("a.txt"), "content").unwrap();
    support::git_ok(local.path(), &["add", "."]);
    support::git_ok(
        local.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ],
    );
    support::git_ok(local.path(), &["branch", "-M", "main"]);
    support::git_ok(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );

    // First push
    push_current_branch(local.path(), "origin", "main").expect("first push should succeed");

    // Push again without changes - should succeed with "everything up-to-date"
    push_current_branch(local.path(), "origin", "main")
        .expect("push with nothing to push should succeed");
}
