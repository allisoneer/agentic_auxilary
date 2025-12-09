//! Integration tests for GitSync with worktree support.
//! These tests verify that sync operations work correctly from linked worktrees.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_sync_worktree

use std::fs;
use std::process::Command;
use tempfile::TempDir;

use thoughts_tool::git::sync::GitSync;

#[tokio::test]
async fn sync_from_worktree_fetches_and_pushes() {
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

    // Create main repo
    let main = TempDir::new().unwrap();
    assert!(
        Command::new("git")
            .args(["init"])
            .current_dir(main.path())
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(main.path())
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
    fs::write(main.path().join("x.txt"), "x").unwrap();
    assert!(
        Command::new("git")
            .current_dir(main.path())
            .args(["add", "."])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(main.path())
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "base"
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(main.path())
            .args(["branch", "-M", "main"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(main.path())
            .args(["push", "-u", "origin", "main"])
            .status()
            .unwrap()
            .success()
    );

    // Create worktree
    let wt_path = main.path().join("wt");
    assert!(
        Command::new("git")
            .current_dir(main.path())
            .args([
                "worktree",
                "add",
                wt_path.to_str().unwrap(),
                "-b",
                "wt-branch",
                "main"
            ])
            .status()
            .unwrap()
            .success()
    );

    // Add a file in the worktree
    fs::write(wt_path.join("y.txt"), "y").unwrap();

    // Sync from worktree
    let sync = GitSync::new(&wt_path, None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify the worktree branch was pushed
    let out = Command::new("git")
        .args([
            "ls-remote",
            remote.path().to_str().unwrap(),
            "refs/heads/wt-branch",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("refs/heads/wt-branch"));
}

#[tokio::test]
async fn sync_stages_and_commits_changes() {
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
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
    fs::write(repo.path().join("initial.txt"), "initial").unwrap();
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
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["branch", "-M", "main"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["push", "-u", "origin", "main"])
            .status()
            .unwrap()
            .success()
    );

    // Get initial commit count
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    let initial_count: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();

    // Add new file
    fs::write(repo.path().join("new.txt"), "new content").unwrap();

    // Sync
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify commit was created
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .unwrap();
    let final_count: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();

    assert_eq!(final_count, initial_count + 1);

    // Verify file is in the commit
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["show", "--name-only", "--format=", "HEAD"])
        .output()
        .unwrap();
    let files = String::from_utf8_lossy(&out.stdout);
    assert!(files.contains("new.txt"));
}

#[tokio::test]
async fn sync_no_changes_no_commit() {
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
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["remote", "add", "origin", remote.path().to_str().unwrap()])
            .status()
            .unwrap()
            .success()
    );
    fs::write(repo.path().join("initial.txt"), "initial").unwrap();
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
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["branch", "-M", "main"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(repo.path())
            .args(["push", "-u", "origin", "main"])
            .status()
            .unwrap()
            .success()
    );

    // Get initial HEAD
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let initial_head = String::from_utf8_lossy(&out.stdout).trim().to_string();

    // Sync without any changes
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify no new commit
    let out = Command::new("git")
        .current_dir(repo.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let final_head = String::from_utf8_lossy(&out.stdout).trim().to_string();

    assert_eq!(initial_head, final_head);
}
