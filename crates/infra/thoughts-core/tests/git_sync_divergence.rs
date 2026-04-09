#![expect(clippy::expect_used, reason = "Tests should panic on failure")]
//! Integration tests for git sync JSONL smart-merge during rebase conflicts.
//! Run with: `just test-integration`
//!
//! Note: Divergence-state tests are unit tests in src/git/sync.rs because they need
//! access to the crate-private `check_divergence()` method. This integration test
//! exercises the full sync flow through the public `GitSync::sync()` API.

mod support;

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

use thoughts_tool::git::sync::GitSync;

fn ensure_remote_head_points_to_main(remote: &TempDir) {
    let bare_repo = git2::Repository::open(remote.path()).expect("open bare remote");
    bare_repo
        .set_head("refs/heads/main")
        .expect("set bare HEAD to main");
}

/// Test: JSONL smart-merge during rebase conflict.
/// Creates divergent commits on a `tool_logs` JSONL file, runs `sync()`, and asserts
/// the merged file contains entries from both sides with correct collision semantics.
#[ignore = "integration test - run with: just test-integration"]
#[tokio::test]
async fn sync_jsonl_smart_merge_on_conflict() {
    // 1. Create bare remote
    let remote = TempDir::new().unwrap();
    support::git_ok(remote.path(), &["init", "--bare"]);

    // 2. Create local repo
    let local = TempDir::new().unwrap();
    support::git_ok(local.path(), &["init"]);

    // 3. Create branch/logs/tool_logs_base.jsonl with base entry
    let logs_dir = local.path().join("branch/logs");
    fs::create_dir_all(&logs_dir).unwrap();
    let jsonl_path = logs_dir.join("tool_logs_base.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"call_id":"base","started_at":"2025-01-01T10:00:00Z","tool":"init"}"#,
    )
    .unwrap();
    fs::write(local.path().join("readme.txt"), "readme").unwrap();

    // 4. Initial commit and push
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
            "initial",
        ],
    );
    // Normalize branch name (git init may create master or main depending on config)
    support::git_ok(local.path(), &["branch", "-M", "main"]);
    support::git_ok(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    support::git_ok(local.path(), &["push", "-u", "origin", "main"]);

    // Fix bare remote HEAD to point to main (otherwise clone gets detached HEAD)
    // Bare repos initialized with `git init --bare` keep HEAD at refs/heads/master
    // and don't update it on push. Real Git servers set HEAD appropriately.
    ensure_remote_head_points_to_main(&remote);

    // 5. Clone to second location (simulating another client)
    let other = TempDir::new().unwrap();
    support::git_ok(
        other.path(),
        &["clone", remote.path().to_str().unwrap(), "."],
    );

    // 6. In other clone: REPLACE base line with remote_modified AND add remote_only entry
    // (This creates a conflict on the base line, triggering the smart JSONL merge)
    let other_jsonl = other.path().join("branch/logs/tool_logs_base.jsonl");
    // Replace original "init" with "remote_modified" for the base entry
    let new_content = r#"{"call_id":"base","started_at":"2025-01-01T10:00:00Z","tool":"remote_modified"}
{"call_id":"remote_only","started_at":"2025-01-01T11:00:00Z","tool":"remote"}"#;
    fs::write(&other_jsonl, new_content).unwrap();
    support::git_ok(other.path(), &["add", "."]);
    support::git_ok(
        other.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "remote change",
        ],
    );
    support::git_ok(other.path(), &["push"]);

    // 7. Back in original local: REPLACE base line with local_override AND add local_only entry
    // (do NOT commit yet — sync() will stage and commit)
    // Both sides now modify the base line, creating a conflict that triggers smart merge
    let new_local_content = r#"{"call_id":"base","started_at":"2025-01-01T10:00:00Z","tool":"local_override"}
{"call_id":"local_only","started_at":"2025-01-01T12:00:00Z","tool":"local"}"#;
    fs::write(&jsonl_path, new_local_content).unwrap();

    // 8. Run sync - this should trigger rebase with smart-merge
    let sync = GitSync::new(local.path(), None).unwrap();
    sync.sync("test").await.unwrap();

    // 9. Verify merged content
    let merged = fs::read_to_string(&jsonl_path).unwrap();

    // Remote-only entry present
    assert!(
        merged.contains("remote_only"),
        "should contain remote_only entry"
    );
    // Local-only entry present
    assert!(
        merged.contains("local_only"),
        "should contain local_only entry"
    );
    // Local wins on collision — "local_override" not "remote_modified"
    assert!(
        merged.contains("local_override"),
        "should contain local_override (local wins)"
    );
    assert!(
        !merged.contains(r#""tool":"remote_modified""#),
        "should NOT contain remote_modified (local wins on collision)"
    );

    // Verify chronological ordering by started_at
    let lines: Vec<&str> = merged.lines().collect();
    assert!(
        lines.len() >= 3,
        "should have at least 3 lines, got {}",
        lines.len()
    );
    // base (10:00) < remote_only (11:00) < local_only (12:00)
    assert!(lines[0].contains("base"), "first line should be base entry");
    assert!(
        lines[1].contains("remote_only"),
        "second line should be remote_only"
    );
    assert!(
        lines[2].contains("local_only"),
        "third line should be local_only"
    );
}

#[ignore = "integration test - run with: just test-integration"]
#[tokio::test]
async fn sync_remote_behind_with_local_uncommitted_creates_one_final_commit() {
    let remote = TempDir::new().unwrap();
    support::git_ok(remote.path(), &["init", "--bare"]);

    let local = TempDir::new().unwrap();
    support::git_ok(local.path(), &["init"]);
    fs::write(local.path().join("base.txt"), "base\n").unwrap();
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
            "initial",
        ],
    );
    support::git_ok(local.path(), &["branch", "-M", "main"]);
    support::git_ok(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    support::git_ok(local.path(), &["push", "-u", "origin", "main"]);
    ensure_remote_head_points_to_main(&remote);

    let other = TempDir::new().unwrap();
    support::git_ok(
        other.path(),
        &["clone", remote.path().to_str().unwrap(), "."],
    );
    fs::write(other.path().join("remote.txt"), "remote\n").unwrap();
    support::git_ok(other.path(), &["add", "."]);
    support::git_ok(
        other.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "remote change",
        ],
    );
    let remote_parent = support::git_stdout(other.path(), &["rev-parse", "HEAD"]);
    support::git_ok(other.path(), &["push"]);

    fs::write(local.path().join("local.txt"), "local\n").unwrap();

    let sync = GitSync::new(local.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    let head_message = support::git_stdout(local.path(), &["log", "-1", "--pretty=%s"]);
    assert_eq!(head_message, "Auto-sync thoughts for test-mount");

    let parent = support::git_stdout(local.path(), &["rev-parse", "HEAD^"]);
    assert_eq!(parent, remote_parent);

    let auto_sync_count: i32 = support::git_stdout(
        local.path(),
        &[
            "rev-list",
            "--count",
            "--grep=^Auto-sync thoughts for test-mount$",
            "HEAD",
        ],
    )
    .parse()
    .unwrap();
    assert_eq!(auto_sync_count, 1);
}

#[ignore = "integration test - run with: just test-integration"]
#[tokio::test]
async fn sync_retries_once_after_race_like_push_rejection() {
    let remote = TempDir::new().unwrap();
    support::git_ok(remote.path(), &["init", "--bare"]);

    let local = TempDir::new().unwrap();
    support::git_ok(local.path(), &["init"]);
    fs::write(local.path().join("base.txt"), "base\n").unwrap();
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
            "initial",
        ],
    );
    support::git_ok(local.path(), &["branch", "-M", "main"]);
    support::git_ok(
        local.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    support::git_ok(local.path(), &["push", "-u", "origin", "main"]);
    ensure_remote_head_points_to_main(&remote);

    let hook_path = remote.path().join("hooks/pre-receive");
    let marker_path = remote.path().join("hooks/race_once_marker");
    fs::write(
        &hook_path,
        format!(
            "#!/bin/sh\nMARKER='{}'\nif [ ! -f \"$MARKER\" ]; then\n  touch \"$MARKER\"\n  echo 'remote: ! [rejected] HEAD -> main (fetch first)' >&2\n  echo 'remote: error: failed to push some refs' >&2\n  exit 1\nfi\nexit 0\n",
            marker_path.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms).unwrap();
    }

    fs::write(local.path().join("retry.txt"), "retry\n").unwrap();

    let sync = GitSync::new(local.path(), None).unwrap();
    sync.sync("test-mount").await.unwrap();

    assert!(
        marker_path.exists(),
        "first push attempt should have been rejected"
    );

    let local_head = support::git_stdout(local.path(), &["rev-parse", "HEAD"]);
    let remote_head = support::git_stdout(remote.path(), &["rev-parse", "refs/heads/main"]);
    assert_eq!(local_head, remote_head);
}
