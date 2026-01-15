//! Integration tests for gitoxide-based clone operations.
//! These tests verify that clone_repository works correctly with gitoxide.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_clone_gitoxide

mod support;

use std::fs;
use tempfile::TempDir;

use thoughts_tool::git::clone::{CloneOptions, clone_repository};

/// Create a bare git remote with an initial commit for testing.
fn init_bare_remote_with_commit() -> (TempDir, String) {
    let remote_dir = TempDir::new().unwrap();
    let remote_path = remote_dir.path().to_path_buf();

    // Init bare repo
    support::git_ok(&remote_path, &["init", "--bare", "."]);

    // Create a working repo, add content, push to bare remote
    let work = TempDir::new().unwrap();
    support::git_ok(work.path(), &["init"]);

    fs::write(work.path().join("README.md"), "hello").unwrap();

    support::git_ok(work.path(), &["add", "."]);
    support::git_ok(
        work.path(),
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
    support::git_ok(work.path(), &["branch", "-M", "main"]);
    support::git_ok(
        work.path(),
        &["remote", "add", "origin", remote_path.to_str().unwrap()],
    );
    support::git_ok(
        work.path(),
        &["push", "-u", "origin", "HEAD:refs/heads/main"],
    );

    (remote_dir, remote_path.to_string_lossy().into())
}

#[test]
fn clone_with_gitoxide_from_file_remote() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    let (_remote_guard, remote_path) = init_bare_remote_with_commit();
    let target = TempDir::new().unwrap();

    let opts = CloneOptions {
        url: remote_path,
        target_path: target.path().join("cloned"),
        branch: Some("main".to_string()),
    };
    clone_repository(&opts).expect("clone should succeed");

    assert!(target.path().join("cloned/.git").exists());
    assert!(target.path().join("cloned/README.md").exists());
    let content = fs::read_to_string(target.path().join("cloned/README.md")).unwrap();
    assert_eq!(content, "hello");
}

#[test]
fn clone_creates_parent_directories() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    let (_remote_guard, remote_path) = init_bare_remote_with_commit();
    let target = TempDir::new().unwrap();

    let opts = CloneOptions {
        url: remote_path,
        target_path: target.path().join("deep/nested/path/cloned"),
        branch: Some("main".to_string()),
    };
    clone_repository(&opts).expect("clone should succeed");

    assert!(target.path().join("deep/nested/path/cloned/.git").exists());
}

#[test]
fn clone_fails_on_nonempty_directory() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    let (_remote_guard, remote_path) = init_bare_remote_with_commit();
    let target = TempDir::new().unwrap();
    let target_dir = target.path().join("cloned");
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("existing.txt"), "content").unwrap();

    let opts = CloneOptions {
        url: remote_path,
        target_path: target_dir,
        branch: Some("main".to_string()),
    };
    let result = clone_repository(&opts);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not empty"));
}
