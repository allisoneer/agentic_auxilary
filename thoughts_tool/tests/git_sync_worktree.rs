//! Integration tests for GitSync with worktree support.
//! These tests verify that sync operations work correctly from linked worktrees.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_sync_worktree

mod support;

use std::fs;
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
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create main repo
    let main = TempDir::new().unwrap();
    support::git_ok(main.path(), &["init"]);
    support::git_ok(
        main.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    fs::write(main.path().join("x.txt"), "x").unwrap();
    support::git_ok(main.path(), &["add", "."]);
    support::git_ok(
        main.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "base",
        ],
    );
    support::git_ok(main.path(), &["branch", "-M", "main"]);
    support::git_ok(main.path(), &["push", "-u", "origin", "main"]);

    // Create worktree
    let wt_path = main.path().join("wt");
    support::git_ok(
        main.path(),
        &[
            "worktree",
            "add",
            wt_path.to_str().unwrap(),
            "-b",
            "wt-branch",
            "main",
        ],
    );

    // Add a file in the worktree
    fs::write(wt_path.join("y.txt"), "y").unwrap();

    // Sync from worktree
    let sync = GitSync::new(&wt_path, None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify the worktree branch was pushed
    let stdout = support::git_stdout(remote.path(), &["ls-remote", ".", "refs/heads/wt-branch"]);
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
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create repo with initial commit
    let repo = TempDir::new().unwrap();
    support::git_ok(repo.path(), &["init"]);
    support::git_ok(
        repo.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    fs::write(repo.path().join("initial.txt"), "initial").unwrap();
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
    support::git_ok(repo.path(), &["branch", "-M", "main"]);
    support::git_ok(repo.path(), &["push", "-u", "origin", "main"]);

    // Get initial commit count
    let initial_count: i32 = support::git_stdout(repo.path(), &["rev-list", "--count", "HEAD"])
        .parse()
        .unwrap();

    // Add new file
    fs::write(repo.path().join("new.txt"), "new content").unwrap();

    // Sync
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify commit was created
    let final_count: i32 = support::git_stdout(repo.path(), &["rev-list", "--count", "HEAD"])
        .parse()
        .unwrap();

    assert_eq!(final_count, initial_count + 1);

    // Verify file is in the commit
    let files = support::git_stdout(repo.path(), &["show", "--name-only", "--format=", "HEAD"]);
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
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create repo with initial commit
    let repo = TempDir::new().unwrap();
    support::git_ok(repo.path(), &["init"]);
    support::git_ok(
        repo.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    fs::write(repo.path().join("initial.txt"), "initial").unwrap();
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
    support::git_ok(repo.path(), &["branch", "-M", "main"]);
    support::git_ok(repo.path(), &["push", "-u", "origin", "main"]);

    // Get initial HEAD
    let initial_head = support::git_stdout(repo.path(), &["rev-parse", "HEAD"]);

    // Sync without any changes
    let sync = GitSync::new(repo.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    // Verify no new commit
    let final_head = support::git_stdout(repo.path(), &["rev-parse", "HEAD"]);

    assert_eq!(initial_head, final_head);
}
