//! Integration tests for shell git-based fetch operations used by pull_ff_only.
//! These tests verify that pull_ff_only fetches via system git and fast-forwards with git2.
//! Run with: THOUGHTS_INTEGRATION_TESTS=1 cargo test --test git_fetch_gitoxide

mod support;

use std::fs;
use tempfile::TempDir;

use git2::Repository;
use thoughts_tool::git::pull::pull_ff_only;
use thoughts_tool::git::utils::is_worktree_dirty;

#[test]
fn fetch_and_fast_forward() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create bare remote
    let remote = TempDir::new().unwrap();
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create producer repo
    let producer = TempDir::new().unwrap();
    support::git_ok(producer.path(), &["init"]);
    fs::write(producer.path().join("a.txt"), "one").unwrap();
    support::git_ok(producer.path(), &["add", "."]);
    support::git_ok(
        producer.path(),
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
    support::git_ok(producer.path(), &["branch", "-M", "main"]);
    support::git_ok(
        producer.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    support::git_ok(producer.path(), &["push", "-u", "origin", "main"]);

    // Ensure the bare "server" advertises a valid default branch (HEAD -> refs/heads/main).
    // Freshly initialized bare repos keep HEAD at refs/heads/master (or user's init.defaultBranch)
    // and do not update it on push. Real Git servers set HEAD appropriately so clones get a branch
    // instead of a detached HEAD. Make our fake server behave like a real one.
    {
        let bare_repo = Repository::open(remote.path()).expect("open bare remote");
        bare_repo
            .set_head("refs/heads/main")
            .expect("set bare HEAD to main");
    }

    // Clone to consumer
    let consumer = TempDir::new().unwrap();
    support::git_ok(
        consumer.path(),
        &["clone", remote.path().to_str().unwrap(), "work"],
    );

    // Producer adds a new file
    fs::write(producer.path().join("b.txt"), "two").unwrap();
    support::git_ok(producer.path(), &["add", "."]);
    support::git_ok(
        producer.path(),
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
    support::git_ok(producer.path(), &["push"]);

    // Consumer fetches and fast-forwards using our gitoxide-based pull
    let work = consumer.path().join("work");
    pull_ff_only(&work, "origin", Some("main")).expect("fetch and ff should succeed");

    // Verify the new file is present
    assert!(work.join("b.txt").exists());
    let content = fs::read_to_string(work.join("b.txt")).unwrap();
    assert_eq!(content, "two");

    // Verify working tree is clean after fast-forward (regression test for v0.4.0 bug)
    let repo = Repository::open(&work).expect("open consumer repo");
    assert!(
        !is_worktree_dirty(&repo).expect("status check failed"),
        "worktree should be clean after fast-forward"
    );

    // Verify HEAD matches upstream
    let head_oid = repo.refname_to_id("HEAD").expect("HEAD oid");
    let upstream_oid = repo
        .refname_to_id("refs/remotes/origin/main")
        .expect("upstream oid");
    assert_eq!(
        head_oid, upstream_oid,
        "HEAD should match upstream after FF"
    );
}

#[test]
fn fetch_already_up_to_date() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create bare remote
    let remote = TempDir::new().unwrap();
    support::git_ok(remote.path(), &["init", "--bare", "."]);

    // Create and push initial commit
    let producer = TempDir::new().unwrap();
    support::git_ok(producer.path(), &["init"]);
    fs::write(producer.path().join("a.txt"), "one").unwrap();
    support::git_ok(producer.path(), &["add", "."]);
    support::git_ok(
        producer.path(),
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
    support::git_ok(producer.path(), &["branch", "-M", "main"]);
    support::git_ok(
        producer.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    support::git_ok(producer.path(), &["push", "-u", "origin", "main"]);

    // Ensure the bare "server" advertises a valid default branch (HEAD -> refs/heads/main).
    // See comment in fetch_and_fast_forward for why this is necessary in test environments.
    {
        let bare_repo = Repository::open(remote.path()).expect("open bare remote");
        bare_repo
            .set_head("refs/heads/main")
            .expect("set bare HEAD to main");
    }

    // Clone to consumer
    let consumer = TempDir::new().unwrap();
    support::git_ok(
        consumer.path(),
        &["clone", remote.path().to_str().unwrap(), "work"],
    );

    // Pull without any new changes - should succeed
    let work = consumer.path().join("work");
    pull_ff_only(&work, "origin", Some("main")).expect("pull should succeed when up to date");

    // Verify working tree remains clean when already up to date
    let repo = Repository::open(&work).expect("open consumer repo");
    assert!(
        !is_worktree_dirty(&repo).expect("status check failed"),
        "worktree should remain clean when already up to date"
    );
}

#[test]
fn fetch_with_no_remote_is_ok() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1");
        return;
    }

    // Create a local-only repo without origin
    let repo = TempDir::new().unwrap();
    support::git_ok(repo.path(), &["init"]);
    fs::write(repo.path().join("a.txt"), "local").unwrap();
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
            "local",
        ],
    );

    // Pull without origin should succeed (no-op)
    pull_ff_only(repo.path(), "origin", Some("main")).expect("pull should succeed without remote");
}
