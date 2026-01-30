//! HTTPS clone smoke tests for gitoxide-based clone operations.
//! Run with:
//!   THOUGHTS_INTEGRATION_TESTS=1 THOUGHTS_NETWORK_TESTS=1 cargo test --test git_clone_https

use tempfile::TempDir;
use thoughts_tool::git::clone::{CloneOptions, clone_repository};

fn should_run() -> bool {
    std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() == Some("1")
        && std::env::var("THOUGHTS_NETWORK_TESTS").ok().as_deref() == Some("1")
}

#[test]
fn https_clone_github_smoke() {
    if !should_run() {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1 and THOUGHTS_NETWORK_TESTS=1");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let opts = CloneOptions {
        url: "https://github.com/octocat/Hello-World.git".to_string(),
        target_path: tmp.path().join("hello-world"),
        branch: None, // use default branch of the repo
    };
    clone_repository(&opts).expect("HTTPS clone from GitHub should succeed");
    assert!(tmp.path().join("hello-world/.git").exists());
}

#[test]
fn https_clone_gitlab_smoke() {
    if !should_run() {
        eprintln!("skipping; set THOUGHTS_INTEGRATION_TESTS=1 and THOUGHTS_NETWORK_TESTS=1");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let opts = CloneOptions {
        url: "https://gitlab.com/gitlab-org/gitlab-test.git".to_string(),
        target_path: tmp.path().join("gitlab-test"),
        branch: None,
    };
    clone_repository(&opts).expect("HTTPS clone from GitLab should succeed");
    assert!(tmp.path().join("gitlab-test/.git").exists());
}
