//! Edge case integration tests for git operations.
//! These tests verify handling of edge cases like detached HEAD, missing remotes, etc.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_edge_cases

mod support;

use std::fs;
use tempfile::TempDir;

use thoughts_tool::git::pull::pull_ff_only;
use thoughts_tool::git::sync::GitSync;

#[test]
fn detached_head_fetch_noop() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create repo with a commit
    let repo = TempDir::new().unwrap();
    support::git_ok(repo.path(), &["init"]);
    fs::write(repo.path().join("a.txt"), "a").unwrap();
    support::git_ok(repo.path(), &["add", "."]);
    support::git_ok(
        repo.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "c",
        ],
    );

    // Get HEAD SHA
    let head = support::git_stdout(repo.path(), &["rev-parse", "HEAD"]);

    // Detach HEAD
    support::git_ok(repo.path(), &["checkout", "--detach", &head]);

    // pull_ff_only should handle missing remote gracefully
    let result = pull_ff_only(repo.path(), "origin", Some("main"));
    assert!(result.is_ok());
}

#[tokio::test]
async fn sync_without_remote_is_ok() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create repo without remote
    let repo = TempDir::new().unwrap();
    support::git_ok(repo.path(), &["init"]);
    fs::write(repo.path().join("a.txt"), "a").unwrap();
    support::git_ok(repo.path(), &["add", "."]);
    support::git_ok(
        repo.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "initial",
        ],
    );

    // Add new file
    fs::write(repo.path().join("b.txt"), "b").unwrap();

    // Sync should work (commit locally, skip push)
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify the file was committed
    let files = support::git_stdout(repo.path(), &["show", "--name-only", "--format=", "HEAD"]);
    assert!(files.contains("b.txt"));
}

#[tokio::test]
async fn sync_empty_repo_initial_commit() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create empty repo (no commits)
    let repo = TempDir::new().unwrap();
    support::git_ok(repo.path(), &["init"]);

    // Add a file
    fs::write(repo.path().join("first.txt"), "first").unwrap();

    // Sync should create initial commit
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify commit was created
    let count_str = support::git_stdout(repo.path(), &["rev-list", "--count", "HEAD"]);
    let count: i32 = count_str.parse().unwrap();
    assert_eq!(count, 1);
}

#[test]
fn fetch_no_upstream_branch() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create bare remote with main branch
    let remote = TempDir::new().unwrap();
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create local with different branch
    let local = TempDir::new().unwrap();
    support::git_ok(local.path(), &["init"]);
    fs::write(local.path().join("a.txt"), "a").unwrap();
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
    support::git_ok(local.path(), &["branch", "-M", "feature"]);
    support::git_ok(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );

    // Try to pull main which doesn't exist on remote
    let result = pull_ff_only(local.path(), "origin", Some("main"));
    assert!(result.is_ok()); // Should succeed, just not do anything
}

#[tokio::test]
async fn sync_subpath_only_commits_subpath() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create repo with initial commit
    let repo = TempDir::new().unwrap();
    support::git_ok(repo.path(), &["init"]);
    fs::write(repo.path().join("root.txt"), "root").unwrap();
    fs::create_dir_all(repo.path().join("subdir")).unwrap();
    fs::write(repo.path().join("subdir/sub.txt"), "sub").unwrap();
    support::git_ok(repo.path(), &["add", "."]);
    support::git_ok(
        repo.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "initial",
        ],
    );

    // Add files in both locations
    fs::write(repo.path().join("root2.txt"), "root2").unwrap();
    fs::write(repo.path().join("subdir/sub2.txt"), "sub2").unwrap();

    // Sync with subpath
    let sync = GitSync::new(repo.path(), Some("subdir".to_string())).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify only subpath file was committed
    let files = support::git_stdout(repo.path(), &["show", "--name-only", "--format=", "HEAD"]);
    assert!(files.contains("subdir/sub2.txt"));
    assert!(!files.contains("root2.txt"));

    // root2.txt should still be unstaged
    let status = support::git_stdout(repo.path(), &["status", "--porcelain"]);
    assert!(status.contains("root2.txt"));
}
