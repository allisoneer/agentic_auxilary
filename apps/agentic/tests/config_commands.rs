//! Integration tests for agentic config commands.

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use tempfile::TempDir;

fn agentic_cmd() -> Command {
    cargo_bin_cmd!("agentic")
}

/// Environment variable for test isolation of config paths.
const CONFIG_DIR_TEST_VAR: &str = "__AGENTIC_CONFIG_DIR_FOR_TESTS";

/// Create an agentic command with isolated config directory.
///
/// This sets the test hook env var via `cmd.env()` (command-scoped, not process-global)
/// and removes env vars that might affect test assertions.
fn agentic_cmd_isolated(config_dir: &std::path::Path) -> Command {
    let mut cmd = agentic_cmd();
    cmd.env(CONFIG_DIR_TEST_VAR, config_dir);
    // Prevent developer/CI env from affecting assertions
    for k in [
        "AGENTIC_SUBAGENTS_LOCATOR_MODEL",
        "AGENTIC_SUBAGENTS_ANALYZER_MODEL",
        "AGENTIC_REASONING_OPTIMIZER_MODEL",
        "AGENTIC_REASONING_EXECUTOR_MODEL",
        "AGENTIC_REASONING_EFFORT",
    ] {
        cmd.env_remove(k);
    }
    cmd
}

#[test]
fn test_schema_outputs_valid_json() {
    let mut cmd = agentic_cmd();
    cmd.args(["config", "schema"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"$defs\"").or(predicate::str::contains("\"definitions\"")),
        )
        .stdout(predicate::str::contains("AgenticConfig"));
}

#[test]
fn test_show_outputs_json() {
    let temp = TempDir::new().unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["config", "show", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"thoughts\""))
        .stdout(predicate::str::contains("\"services\""));
}

#[test]
fn test_init_creates_config_file() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.json");

    // Init should create the file
    let mut cmd = agentic_cmd();
    cmd.current_dir(temp.path())
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    // File should exist
    assert!(config_path.exists());

    // File should be valid JSON
    let content = std::fs::read_to_string(&config_path).unwrap();
    let _: serde_json::Value = serde_json::from_str(&content).unwrap();
}

#[test]
fn test_init_fails_if_exists_without_force() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.json");

    // Create a file first
    std::fs::write(&config_path, "{}").unwrap();

    // Init without force should fail
    let mut cmd = agentic_cmd();
    cmd.current_dir(temp.path())
        .args(["config", "init"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_init_succeeds_with_force() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.json");

    // Create a file first
    std::fs::write(&config_path, "{}").unwrap();

    // Init with force should succeed
    let mut cmd = agentic_cmd();
    cmd.current_dir(temp.path())
        .args(["config", "init", "--force"])
        .assert()
        .success();

    // File should have default config content now
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("\"thoughts\""));
}

#[test]
fn test_validate_succeeds_on_valid_config() {
    let temp = TempDir::new().unwrap();

    let mut cmd = agentic_cmd();
    cmd.args([
        "config",
        "validate",
        "--path",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("valid"));
}

#[test]
fn test_validate_shows_warnings() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.json");

    // Create config with invalid value
    std::fs::write(
        &config_path,
        r#"{"services": {"anthropic": {"base_url": "not-a-url"}}}"#,
    )
    .unwrap();

    let mut cmd = agentic_cmd();
    cmd.args([
        "config",
        "validate",
        "--path",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("warning"));
}

#[test]
fn test_show_with_local_config() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.json");

    // Create local config with custom value (using new subagents config structure)
    std::fs::write(
        &config_path,
        r#"{"subagents": {"locator_model": "custom-locator-model"}}"#,
    )
    .unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["config", "show", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("custom-locator-model"));
}

#[test]
fn test_show_reflects_env_overrides() {
    let temp = TempDir::new().unwrap();

    // Use command-scoped env (not process-global) for test isolation
    let mut cmd = agentic_cmd_isolated(temp.path());
    cmd.env("AGENTIC_SUBAGENTS_LOCATOR_MODEL", "env-override-model")
        .args(["config", "show", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("env-override-model"));
}

#[test]
fn test_version_flag() {
    let mut cmd = agentic_cmd();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("agentic"));
}

#[test]
fn test_help_flag() {
    let mut cmd = agentic_cmd();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("Configuration"));
}

// =============================================================================
// Migrate command tests
// =============================================================================

#[test]
fn test_migrate_dry_run_does_not_write() {
    let temp = TempDir::new().unwrap();
    let legacy_dir = temp.path().join(".thoughts");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    std::fs::write(
        legacy_dir.join("config.json"),
        r#"{"version": "2.0", "mount_dirs": {"thoughts": "t"}}"#,
    )
    .unwrap();

    let mut cmd = agentic_cmd_isolated(temp.path());
    cmd.args([
        "config",
        "migrate",
        "--dry-run",
        "--path",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("\"thoughts\""));

    assert!(!temp.path().join("agentic.json").exists());
}

#[test]
fn test_migrate_creates_file() {
    let temp = TempDir::new().unwrap();
    let legacy_dir = temp.path().join(".thoughts");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    std::fs::write(
        legacy_dir.join("config.json"),
        r#"{"version": "2.0", "mount_dirs": {"thoughts": "docs"}}"#,
    )
    .unwrap();

    let mut cmd = agentic_cmd_isolated(temp.path());
    cmd.args(["config", "migrate", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success();

    assert!(temp.path().join("agentic.json").exists());
}

#[test]
fn test_migrate_fails_if_target_exists() {
    let temp = TempDir::new().unwrap();
    let legacy_dir = temp.path().join(".thoughts");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    std::fs::write(legacy_dir.join("config.json"), r#"{"version": "2.0"}"#).unwrap();
    std::fs::write(temp.path().join("agentic.json"), "{}").unwrap();

    let mut cmd = agentic_cmd_isolated(temp.path());
    cmd.args(["config", "migrate", "--path", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_migrate_fails_if_no_legacy() {
    let temp = TempDir::new().unwrap();

    let mut cmd = agentic_cmd_isolated(temp.path());
    cmd.args(["config", "migrate", "--path", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No legacy config"));
}
