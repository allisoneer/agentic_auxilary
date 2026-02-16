//! Integration tests for agentic config commands.

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serial_test::serial;
use tempfile::TempDir;

fn agentic_cmd() -> Command {
    cargo_bin_cmd!("agentic")
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

    // Create local config with custom value
    std::fs::write(
        &config_path,
        r#"{"models": {"default_model": "custom-model"}}"#,
    )
    .unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["config", "show", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("custom-model"));
}

#[test]
#[serial]
fn test_show_reflects_env_overrides() {
    let temp = TempDir::new().unwrap();

    // Set env var
    // SAFETY: This test runs serially via #[serial] to avoid data races
    unsafe {
        std::env::set_var("AGENTIC_MODEL_DEFAULT", "env-override-model");
    }

    let mut cmd = agentic_cmd();
    let result = cmd
        .args(["config", "show", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success();

    // Clean up env var
    // SAFETY: This test runs serially via #[serial] to avoid data races
    unsafe {
        std::env::remove_var("AGENTIC_MODEL_DEFAULT");
    }

    result.stdout(predicate::str::contains("env-override-model"));
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
