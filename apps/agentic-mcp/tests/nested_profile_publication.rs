#![expect(
    clippy::expect_used,
    reason = "integration fixtures should fail immediately"
)]

use std::process::Command;

fn git(cwd: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git fixture command");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn nested_fixture() -> tempfile::TempDir {
    let repo = tempfile::TempDir::new().expect("temporary repository");
    git(repo.path(), &["init"]);
    git(
        repo.path(),
        &["config", "user.email", "fixture@example.com"],
    );
    git(repo.path(), &["config", "user.name", "Fixture"]);
    std::fs::write(repo.path().join("seed"), "seed").expect("write git fixture");
    git(repo.path(), &["add", "seed"]);
    git(repo.path(), &["commit", "-m", "fixture"]);
    git(repo.path(), &["checkout", "-b", "nested-fixture"]);

    std::fs::create_dir_all(repo.path().join(".thoughts")).expect("create config directory");
    std::fs::write(
        repo.path().join(".thoughts/config.json"),
        r#"{
            "version": "2.0",
            "mount_dirs": {
                "thoughts": "thoughts",
                "context": "context",
                "references": "references"
            },
            "context_mounts": [],
            "references": []
        }"#,
    )
    .expect("write thoughts config");
    repo
}

fn fixture_command(repo: &tempfile::TempDir) -> (tempfile::TempDir, Command) {
    let home = tempfile::TempDir::new().expect("temporary home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_agentic-mcp"));
    command
        .current_dir(repo.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("xdg"));
    (home, command)
}

fn isolated_command() -> (tempfile::TempDir, Command) {
    let home = tempfile::TempDir::new().expect("temporary home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_agentic-mcp"));
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("xdg"));
    (home, command)
}

fn assert_no_published_tools(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Available tools (0):"),
        "{context}: {stderr}"
    );
    assert!(!stderr.contains("  - "), "{context}: {stderr}");
    assert!(output.stdout.is_empty(), "{context}: stdout was not empty");
}

#[test]
fn default_parent_does_not_publish_runtime_only_tools() {
    let (_home, mut command) = isolated_command();
    let output = command
        .args([
            "--allow",
            "workspace_read,workspace_todowrite,thoughts_read_document,thoughts_read_reference",
            "--list-tools",
        ])
        .output()
        .expect("run agentic-mcp");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Available tools (0)"), "{stderr}");
    assert!(output.stdout.is_empty());
}

#[test]
fn codebase_profile_publishes_only_allowlisted_read_capabilities() {
    let (_home, mut command) = isolated_command();
    let output = command
        .args([
            "--nested-profile",
            "codebase",
            "--allow",
            "cli_ls,workspace_read",
            "--list-tools",
        ])
        .output()
        .expect("run nested agentic-mcp");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Available tools (2)"), "{stderr}");
    assert!(stderr.contains("cli_ls"));
    assert!(stderr.contains("workspace_read"));
    assert!(!stderr.contains("  - workspace_edit"));
    assert!(!stderr.contains("  - workspace_apply_patch"));
    assert!(output.stdout.is_empty());
}

#[test]
fn web_profile_has_no_local_read_or_discovery_tools() {
    let (_home, mut command) = isolated_command();
    let output = command
        .args([
            "--nested-profile",
            "web",
            "--allow",
            "web_search,web_fetch,workspace_todowrite",
            "--list-tools",
        ])
        .output()
        .expect("run nested agentic-mcp");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Available tools (3)"), "{stderr}");
    assert!(!stderr.contains("  - workspace_read"));
    assert!(!stderr.contains("  - cli_ls"));
}

#[test]
fn thoughts_profile_resolves_mounted_active_work_without_mutation() {
    let repo = nested_fixture();
    let thoughts = repo.path().join(".thoughts-data/thoughts/nested-fixture");
    std::fs::create_dir_all(&thoughts).expect("create active thoughts fixture");
    std::fs::write(thoughts.join("sentinel.md"), "sentinel").expect("write thoughts fixture");
    let weekly = repo.path().join(".thoughts-data/thoughts/2025-W01");
    std::fs::create_dir_all(&weekly).expect("create weekly sentinel");

    let (_home, mut command) = fixture_command(&repo);
    let output = command
        .args([
            "--nested-profile",
            "thoughts",
            "--allow",
            "cli_ls,thoughts_list_documents,thoughts_read_document",
            "--list-tools",
        ])
        .output()
        .expect("run Thoughts profile");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Available tools (3)"), "{stderr}");
    assert!(stderr.contains("  - cli_ls"));
    assert!(stderr.contains("  - thoughts_list_documents"));
    assert!(stderr.contains("  - thoughts_read_document"));
    assert!(!stderr.contains("  - workspace_todowrite"));
    assert!(!thoughts.join("manifest.json").exists());
    assert!(!thoughts.join("research").exists());
    assert!(weekly.exists());
    assert!(
        !repo
            .path()
            .join(".thoughts-data/thoughts/completed")
            .exists()
    );
    assert!(output.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn thoughts_profile_rejects_symlinked_active_work_root() {
    use std::os::unix::fs::symlink;

    let repo = nested_fixture();
    let outside = tempfile::TempDir::new().expect("outside fixture");
    let thoughts_root = repo.path().join(".thoughts-data/thoughts");
    std::fs::create_dir_all(&thoughts_root).expect("create Thoughts mount fixture");
    symlink(outside.path(), thoughts_root.join("nested-fixture"))
        .expect("create active-work escape symlink");

    let (_home, mut command) = fixture_command(&repo);
    let output = command
        .args([
            "--nested-profile",
            "thoughts",
            "--allow",
            "thoughts_read_document",
            "--list-tools",
        ])
        .output()
        .expect("run symlinked Thoughts profile");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must not be a symlink"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn references_profile_resolves_configured_mount_and_exact_publication() {
    let repo = nested_fixture();
    std::fs::create_dir_all(repo.path().join(".thoughts-data/references"))
        .expect("create References fixture");
    let (_home, mut command) = fixture_command(&repo);
    let output = command
        .args([
            "--nested-profile",
            "references",
            "--allow",
            "cli_glob,thoughts_list_references,thoughts_read_reference,workspace_todowrite",
            "--list-tools",
        ])
        .output()
        .expect("run References profile");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Available tools (4)"), "{stderr}");
    assert!(stderr.contains("  - cli_glob"));
    assert!(stderr.contains("  - thoughts_list_references"));
    assert!(stderr.contains("  - thoughts_read_reference"));
    assert!(stderr.contains("  - workspace_todowrite"));
    assert!(!stderr.contains("  - workspace_read"));
    assert!(output.stdout.is_empty());
}

#[test]
fn references_profile_rejects_unsafe_configured_mount_directory() {
    let repo = nested_fixture();
    std::fs::write(
        repo.path().join(".thoughts/config.json"),
        r#"{
            "version": "2.0",
            "mount_dirs": {
                "thoughts": "thoughts",
                "context": "context",
                "references": "/etc"
            },
            "context_mounts": [],
            "references": []
        }"#,
    )
    .expect("write unsafe config fixture");
    let (_home, mut command) = fixture_command(&repo);
    let output = command
        .args([
            "--nested-profile",
            "references",
            "--allow",
            "thoughts_read_reference",
            "--list-tools",
        ])
        .output()
        .expect("run unsafe References profile");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("Mount directory 'references' must be a single path segment")
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn nested_profile_without_allowlist_publishes_nothing() {
    let (_home, mut command) = isolated_command();
    let output = command
        .args(["--nested-profile", "codebase", "--list-tools"])
        .output()
        .expect("run nested agentic-mcp");
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Available tools (0)"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn nested_profile_without_allowlist_ignores_server_config_allowlist() {
    let config = tempfile::NamedTempFile::new().expect("temporary server config");
    std::fs::write(config.path(), r#"{"allowlist":["cli_ls"]}"#).expect("write server config");
    let (_home, mut command) = isolated_command();
    let output = command
        .arg("--nested-profile")
        .arg("codebase")
        .arg("--server-config")
        .arg(config.path())
        .arg("--list-tools")
        .output()
        .expect("run nested agentic-mcp with server config");
    assert_no_published_tools(&output, "server config bypass");
}

#[test]
fn nested_profile_without_allowlist_ignores_convenience_flags() {
    let (_home, mut command) = isolated_command();
    let output = command
        .args([
            "--nested-profile",
            "codebase",
            "--cli-ls",
            "--cli-grep",
            "--cli-glob",
            "--list-tools",
        ])
        .output()
        .expect("run nested agentic-mcp with convenience flags");
    assert_no_published_tools(&output, "convenience flag bypass");
}

#[test]
fn invalid_nested_allowlist_cannot_be_repaired_by_other_allowlist_sources() {
    let config = tempfile::NamedTempFile::new().expect("temporary server config");
    std::fs::write(config.path(), r#"{"allowlist":["cli_ls"]}"#).expect("write server config");
    let (_home, mut command) = isolated_command();
    let output = command
        .arg("--nested-profile")
        .arg("codebase")
        .arg("--allow")
        .arg("CLI_LS")
        .arg("--server-config")
        .arg(config.path())
        .arg("--cli-ls")
        .arg("--list-tools")
        .output()
        .expect("run nested agentic-mcp with invalid explicit allowlist");
    assert_no_published_tools(&output, "invalid explicit allowlist bypass");
}

#[test]
fn dangerous_nested_allowlist_names_publish_exactly_nothing() {
    for raw in [
        "CLI_LS",
        " cli_ls",
        "cli_ls ",
        "mcp__agentic-mcp__cli_ls",
        "prefix_cli_ls",
        "cli_ls_suffix",
        "unknown",
        "cli_ls,cli_ls",
        "",
    ] {
        let (_home, mut command) = isolated_command();
        let output = command
            .args([
                "--nested-profile",
                "codebase",
                "--allow",
                raw,
                "--list-tools",
            ])
            .output()
            .expect("run malformed nested allowlist");
        assert!(
            output.status.success(),
            "input {raw:?} failed to fail closed"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Available tools (0):"),
            "input {raw:?}: {stderr}"
        );
        assert_no_published_tools(&output, &format!("input {raw:?}"));
    }
}

#[test]
fn nested_profile_rejects_non_git_working_directory() {
    let home = tempfile::TempDir::new().expect("temporary home");
    let cwd = tempfile::TempDir::new().expect("non-git cwd");
    let output = Command::new(env!("CARGO_BIN_EXE_agentic-mcp"))
        .current_dir(cwd.path())
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("xdg"))
        .args([
            "--nested-profile",
            "codebase",
            "--allow",
            "cli_ls",
            "--list-tools",
        ])
        .output()
        .expect("run nested agentic-mcp");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}
