//! Integration tests for shell git-based fetch operations.
//! These tests verify that shell_fetch::fetch works correctly with system git.

use anyhow::Result;
use git2::Repository;
use thoughts_tool::git::shell_fetch;
use thoughts_tool::git::shell_push;

fn skip_if_integration_not_enabled() -> bool {
    std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1")
}

#[test]
fn shell_fetch_fetches_from_local_bare_remote() -> Result<()> {
    if skip_if_integration_not_enabled() {
        eprintln!("Skipping integration test: THOUGHTS_INTEGRATION_TESTS != 1");
        return Ok(());
    }

    // Ensure git is available
    if which::which("git").is_err() {
        eprintln!("Skipping integration test: git not found in PATH");
        return Ok(());
    }

    let tmp = tempfile::TempDir::new()?;
    let base = tmp.path();

    let bare_path = base.join("remote_bare.git");
    let upstream_path = base.join("upstream");
    let local_path = base.join("local");

    // Init bare remote
    Repository::init_bare(&bare_path)?;

    // Init upstream, create a commit on main, push to bare
    {
        let upstream = Repository::init(&upstream_path)?;
        // Write a file
        std::fs::write(upstream_path.join("README.md"), "hello")?;
        // Stage and commit
        let mut idx = upstream.index()?;
        idx.add_path(std::path::Path::new("README.md"))?;
        idx.write()?;
        let tree_id = idx.write_tree()?;
        let tree = upstream.find_tree(tree_id)?;
        let sig = git2::Signature::now("Test", "test@example.com")?;
        upstream.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])?;

        // Rename current branch to 'main' using shell git (avoids git2 force-update HEAD issue)
        std::process::Command::new("git")
            .current_dir(&upstream_path)
            .args(["branch", "-M", "main"])
            .status()?;

        // Add remote pointing to bare and push with shell git
        upstream.remote("origin", bare_path.to_str().unwrap())?;
        shell_push::push_current_branch(&upstream_path, "origin", "main")?;
    }

    // Init local and add same origin
    let local = Repository::init(&local_path)?;
    local.remote("origin", bare_path.to_str().unwrap())?;

    // Fetch using shell fetch
    shell_fetch::fetch(&local_path, "origin")?;

    // Verify remote ref exists
    let local = Repository::open(&local_path)?;
    let remote_ref = local.find_reference("refs/remotes/origin/main")?;
    let _oid = remote_ref
        .target()
        .expect("origin/main should have a target");
    Ok(())
}
