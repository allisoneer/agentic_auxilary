#![expect(clippy::unwrap_used, reason = "Tests should panic on failure")]
//! Integration tests for remote ref discovery.
//! Run with: `just test-integration`

mod support;

use std::fs;
use tempfile::TempDir;

use thoughts_tool::git::remote_refs::discover_remote_refs;

fn init_remote_with_branch_and_tag() -> (TempDir, TempDir, String) {
    let remote_dir = TempDir::new().unwrap();
    let remote_path = remote_dir.path().to_path_buf();
    support::git_ok(&remote_path, &["init", "--bare", "."]);

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
    support::git_ok(
        work.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "tag",
            "-a",
            "v1.0.0",
            "-m",
            "v1.0.0",
        ],
    );
    support::git_ok(work.path(), &["push", "origin", "refs/tags/v1.0.0"]);
    support::git_ok(work.path(), &["checkout", "-b", "feature/demo"]);
    fs::write(work.path().join("README.md"), "feature").unwrap();
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
            "feature",
        ],
    );
    support::git_ok(
        work.path(),
        &["push", "-u", "origin", "HEAD:refs/heads/feature/demo"],
    );

    (remote_dir, work, remote_path.to_string_lossy().into())
}

#[ignore = "integration test - run with: just test-integration"]
#[test]
fn discovers_remote_refs_from_local_bare_remote() {
    let (_remote_guard, work_guard, remote_path) = init_remote_with_branch_and_tag();
    let refs = discover_remote_refs(work_guard.path(), &remote_path).unwrap();

    assert!(refs.iter().any(|r| r.name == "refs/heads/main"));
    assert!(refs.iter().any(|r| r.name == "refs/heads/feature/demo"));

    let tag = refs
        .iter()
        .find(|r| r.name == "refs/tags/v1.0.0")
        .expect("annotated tag ref should be present");
    assert!(tag.oid.is_some());
    assert!(tag.peeled.is_some());
}
