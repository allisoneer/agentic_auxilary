//! Edge case integration tests for git operations.
//! These tests verify handling of edge cases like detached HEAD, missing remotes, etc.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_edge_cases

use std::fs;
use std::process::Command;
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
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(repo.path().join("a.txt"), "a").unwrap();
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "c"
            ])
            .status()
            .unwrap()
            .success()
    );

    // Get HEAD SHA
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();

    // Detach HEAD
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["checkout", "--detach", &head])
            .status()
            .unwrap()
            .success()
    );

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
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(repo.path().join("a.txt"), "a").unwrap();
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "initial"
            ])
            .status()
            .unwrap()
            .success()
    );

    // Add new file
    fs::write(repo.path().join("b.txt"), "b").unwrap();

    // Sync should work (commit locally, skip push)
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify the file was committed
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["show", "--name-only", "--format=", "HEAD"])
        .output()
        .unwrap();
    let files = String::from_utf8_lossy(&out.stdout);
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
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );

    // Add a file
    fs::write(repo.path().join("first.txt"), "first").unwrap();

    // Sync should create initial commit
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify commit was created
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let count: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
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
    assert!(
        Command::new("git")
            .args(["init", "--bare"])
            .arg(remote.path())
            .status()
            .unwrap()
            .success()
    );

    // Create local with different branch
    let local = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(local.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(local.path().join("a.txt"), "a").unwrap();
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
            .args(["branch", "-M", "feature"])
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
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );
    fs::write(repo.path().join("root.txt"), "root").unwrap();
    fs::create_dir_all(repo.path().join("subdir")).unwrap();
    fs::write(repo.path().join("subdir/sub.txt"), "sub").unwrap();
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "initial"
            ])
            .status()
            .unwrap()
            .success()
    );

    // Add files in both locations
    fs::write(repo.path().join("root2.txt"), "root2").unwrap();
    fs::write(repo.path().join("subdir/sub2.txt"), "sub2").unwrap();

    // Sync with subpath
    let sync = GitSync::new(repo.path(), Some("subdir".to_string())).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify only subpath file was committed
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["show", "--name-only", "--format=", "HEAD"])
        .output()
        .unwrap();
    let files = String::from_utf8_lossy(&out.stdout);
    assert!(files.contains("subdir/sub2.txt"));
    assert!(!files.contains("root2.txt"));

    // root2.txt should still be unstaged
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    let status = String::from_utf8_lossy(&out.stdout);
    assert!(status.contains("root2.txt"));
}
