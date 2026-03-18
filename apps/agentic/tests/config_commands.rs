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
    // TODO(2): ENV-to-config mappings need improvement:
    // - Consider a centralized registry instead of this hardcoded array
    // - Mapping logic in loader.rs:apply_env_overrides() should derive from typed config
    // - Reference General-Wisdom/monorepo's #[derive(EnvVars)] pattern for inspiration
    // - Not all config values should necessarily have ENV equivalents; clarify policy
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
fn test_show_outputs_toml() {
    let temp = TempDir::new().unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["config", "show", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("[subagents]"))
        .stdout(predicate::str::contains("[services.anthropic]"));
}

#[test]
fn test_show_json_flag() {
    let temp = TempDir::new().unwrap();

    let mut cmd = agentic_cmd();
    cmd.args([
        "config",
        "show",
        "--json",
        "--path",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    // JSON output should contain typical JSON structure
    .stdout(predicate::str::contains("\"subagents\""))
    .stdout(predicate::str::contains("\"services\""));
}

#[test]
fn test_init_creates_config_file() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.toml");

    // Init should create the file
    let mut cmd = agentic_cmd();
    cmd.current_dir(temp.path())
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    // File should exist
    assert!(config_path.exists());

    // File should be valid TOML
    let content = std::fs::read_to_string(&config_path).unwrap();
    let _: toml::Value = toml::from_str(&content).unwrap();
}

#[test]
fn test_init_fails_if_exists_without_force() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.toml");

    // Create a file first
    std::fs::write(&config_path, "# existing config").unwrap();

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
    let config_path = temp.path().join("agentic.toml");

    // Create a file first
    std::fs::write(&config_path, "# old config").unwrap();

    // Init with force should succeed
    let mut cmd = agentic_cmd();
    cmd.current_dir(temp.path())
        .args(["config", "init", "--force"])
        .assert()
        .success();

    // File should have default config content now
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[subagents]"));
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
    let config_path = temp.path().join("agentic.toml");

    // Create config with invalid value
    std::fs::write(
        &config_path,
        r#"
[services.anthropic]
base_url = "not-a-url"
"#,
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
    let config_path = temp.path().join("agentic.toml");

    // Create local config with custom value
    std::fs::write(
        &config_path,
        r#"
[subagents]
locator_model = "custom-locator-model"
"#,
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

#[test]
fn test_warns_on_unknown_top_level_key() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("agentic.toml");

    // Create config with unknown key
    std::fs::write(
        &config_path,
        r#"
unknown_section = "value"
"#,
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
    .stdout(predicate::str::contains("unknown").or(predicate::str::contains("Unknown")));
}
