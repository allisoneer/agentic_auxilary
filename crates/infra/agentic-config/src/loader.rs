//! Configuration loader with two-layer merge and env overrides.
//!
//! The loading process:
//! 1. Read global config from `~/.config/agentic/agentic.json`
//! 2. Read local config from `./agentic.json`
//! 3. Deep merge at JSON Value level (RFC 7396)
//! 4. Deserialize once into typed AgenticConfig
//! 5. Apply env var overrides (highest precedence)
//! 6. Run advisory validation

use crate::{merge::merge_patch, types::AgenticConfig, validation::AdvisoryWarning};
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Filename for local config.
pub const LOCAL_FILE: &str = "agentic.json";

/// Directory name under config_dir for global config.
pub const GLOBAL_DIR: &str = "agentic";

/// Filename for global config.
pub const GLOBAL_FILE: &str = "agentic.json";

/// Resolved paths for config files.
#[derive(Debug, Clone)]
pub struct AgenticConfigPaths {
    /// Path to local config (./agentic.json).
    pub local: PathBuf,

    /// Path to global config (~/.config/agentic/agentic.json).
    pub global: PathBuf,
}

/// Events that occurred during config loading.
#[derive(Debug, Clone)]
pub enum LoadEvent {
    /// V2 thoughts config was migrated to agentic.json.
    MigratedThoughtsV2 { from: PathBuf, to: PathBuf },
}

/// Result of loading configuration.
#[derive(Debug)]
pub struct LoadedAgenticConfig {
    /// The loaded and merged configuration.
    pub config: AgenticConfig,

    /// Advisory warnings from validation.
    pub warnings: Vec<AdvisoryWarning>,

    /// Events that occurred during loading (e.g., migration).
    pub events: Vec<LoadEvent>,

    /// Resolved config file paths.
    pub paths: AgenticConfigPaths,
}

/// Get the global config file path.
///
/// Returns `~/.config/agentic/agentic.json` on Unix-like systems.
///
/// # Test Hook
///
/// Set `__AGENTIC_CONFIG_DIR_FOR_TESTS` to override the base config directory.
/// This bypasses `dirs::config_dir()` entirely, enabling cross-platform test isolation.
/// The env var is inert in production (never set outside tests).
pub fn global_config_path() -> Result<PathBuf> {
    // Test hook (inert unless env var set)
    if let Ok(override_dir) = std::env::var("__AGENTIC_CONFIG_DIR_FOR_TESTS") {
        return Ok(PathBuf::from(override_dir)
            .join(GLOBAL_DIR)
            .join(GLOBAL_FILE));
    }

    let base = dirs::config_dir().context("Could not determine config dir")?;
    Ok(base.join(GLOBAL_DIR).join(GLOBAL_FILE))
}

/// Get the local config file path for a given directory.
pub fn local_config_path(local_dir: &Path) -> PathBuf {
    local_dir.join(LOCAL_FILE)
}

/// Load and merge configuration from global and local files.
///
/// # Precedence (lowest to highest)
/// 1. Default values
/// 2. Global config (`~/.config/agentic/agentic.json`)
/// 3. Local config (`./agentic.json`)
/// 4. Environment variables
///
/// # Legacy fallback (read-only)
/// If `./agentic.json` doesn't exist but `./.thoughts/config.json` does (V2 format),
/// the legacy config is loaded in-memory and mapped to agentic.json structure.
/// No files are written during config loading.
pub fn load_merged(local_dir: &Path) -> Result<LoadedAgenticConfig> {
    let global_path = global_config_path()?;
    let local_path = local_config_path(local_dir);

    // No auto-migration; loader is strictly read-only.
    let events = vec![];

    // Read configs as JSON Values
    let global_v = read_json_object_or_empty(&global_path)?;

    let legacy_path = crate::migration::should_migrate(local_dir, &local_path);
    let (local_v, legacy_used) = if let Some(legacy_path) = legacy_path {
        let mapped = read_legacy_v2_as_agentic_object(&legacy_path)?;
        (mapped, Some(legacy_path))
    } else {
        (read_json_object_or_empty(&local_path)?, None)
    };

    // Merge: global as base, local as patch
    let merged = merge_patch(global_v, local_v);

    // Detect deprecated keys from raw JSON (before deserialization)
    let mut warnings = crate::validation::detect_deprecated_keys(&merged);

    if let Some(legacy_path) = legacy_used.as_deref() {
        warnings.push(legacy_config_warning(legacy_path));
    }

    // Deserialize to typed config
    let mut cfg: AgenticConfig =
        serde_json::from_value(merged).context("Failed to deserialize merged agentic config")?;

    // Apply env var overrides (highest precedence)
    apply_env_overrides(&mut cfg);

    // Run advisory validation and add to warnings
    warnings.extend(crate::validation::validate(&cfg));

    Ok(LoadedAgenticConfig {
        config: cfg,
        warnings,
        events,
        paths: AgenticConfigPaths {
            local: local_path,
            global: global_path,
        },
    })
}

/// Apply environment variable overrides to the config.
fn apply_env_overrides(cfg: &mut AgenticConfig) {
    // Service URLs
    if let Some(v) = env_trimmed("ANTHROPIC_BASE_URL") {
        cfg.services.anthropic.base_url = v;
    }
    if let Some(v) = env_trimmed("EXA_BASE_URL") {
        cfg.services.exa.base_url = v;
    }

    // API keys (env-only)
    if let Some(k) = env_trimmed("ANTHROPIC_API_KEY") {
        cfg.services.anthropic.api_key = Some(secrecy::SecretString::from(k));
    }
    if let Some(k) = env_trimmed("EXA_API_KEY") {
        cfg.services.exa.api_key = Some(secrecy::SecretString::from(k));
    }

    // Subagents model overrides
    if let Some(v) = env_trimmed("AGENTIC_SUBAGENTS_LOCATOR_MODEL") {
        cfg.subagents.locator_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_SUBAGENTS_ANALYZER_MODEL") {
        cfg.subagents.analyzer_model = v;
    }

    // Reasoning model overrides
    if let Some(v) = env_trimmed("AGENTIC_REASONING_OPTIMIZER_MODEL") {
        cfg.reasoning.optimizer_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_REASONING_EXECUTOR_MODEL") {
        cfg.reasoning.executor_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_REASONING_EFFORT") {
        cfg.reasoning.reasoning_effort = Some(v);
    }

    // Logging overrides
    if let Some(v) = env_trimmed("AGENTIC_LOG_LEVEL") {
        cfg.logging.level = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_LOG_JSON") {
        cfg.logging.json = v.to_lowercase() == "true" || v == "1";
    }
}

/// Helper to read and normalize an env var (trim + filter empty).
fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Read legacy V2 config and map it to agentic.json format (in-memory).
fn read_legacy_v2_as_agentic_object(legacy_path: &Path) -> Result<Value> {
    let v2 = crate::migration::read_legacy_v2(legacy_path)?;
    let mapped = crate::migration::map_v2_to_agentic_value(v2);

    match mapped {
        Value::Object(_) => Ok(mapped),
        _ => anyhow::bail!(
            "Internal error: mapped legacy config must be a JSON object (source: {})",
            legacy_path.display()
        ),
    }
}

/// Create an advisory warning for legacy config usage.
fn legacy_config_warning(legacy_path: &Path) -> AdvisoryWarning {
    AdvisoryWarning::new(
        "legacy_config.used",
        "$",
        format!(
            "Using legacy config at {}. To migrate: `agentic config show > agentic.json`",
            legacy_path.display()
        ),
    )
}

/// Read a JSON file as a Value, returning empty object if file doesn't exist.
fn read_json_object_or_empty(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(Value::Object(Default::default()));
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;

    let v: Value = serde_json::from_str(&raw)
        .with_context(|| format!("Invalid JSON in {}", path.display()))?;

    match v {
        Value::Object(_) => Ok(v),
        _ => anyhow::bail!("Config root must be a JSON object: {}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::EnvGuard;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Env var name for test isolation of global config path.
    const CONFIG_DIR_TEST_VAR: &str = "__AGENTIC_CONFIG_DIR_FOR_TESTS";

    #[test]
    #[serial]
    fn test_load_no_files_returns_defaults() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        let loaded = load_merged(temp.path()).unwrap();

        // Should get default values
        assert_eq!(
            loaded.config.services.anthropic.base_url,
            "https://api.anthropic.com"
        );
        assert_eq!(loaded.config.thoughts.mount_dirs.thoughts, "thoughts");
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    #[serial]
    fn test_load_local_only() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        let local_path = temp.path().join(LOCAL_FILE);
        std::fs::write(
            &local_path,
            r#"{"thoughts": {"mount_dirs": {"thoughts": "my-thoughts"}}}"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(loaded.config.thoughts.mount_dirs.thoughts, "my-thoughts");
        // Other fields should be defaults
        assert_eq!(loaded.config.thoughts.mount_dirs.context, "context");
    }

    #[test]
    #[serial]
    fn test_local_overrides_global() {
        let temp = TempDir::new().unwrap();

        // Point global config to our temp directory
        let global_base = temp.path().join("global_config");
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, &global_base);

        // Create global config
        let global_dir = global_base.join(GLOBAL_DIR);
        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::write(
            global_dir.join(GLOBAL_FILE),
            r#"{"subagents": {"locator_model": "global-model", "analyzer_model": "global-analyzer"}}"#,
        )
        .unwrap();

        // Create local config that overrides locator_model but not analyzer_model
        let local_dir = temp.path().join("local_repo");
        std::fs::create_dir_all(&local_dir).unwrap();
        std::fs::write(
            local_dir.join(LOCAL_FILE),
            r#"{"subagents": {"locator_model": "local-model"}}"#,
        )
        .unwrap();

        let loaded = load_merged(&local_dir).unwrap();
        // Local wins for locator_model
        assert_eq!(loaded.config.subagents.locator_model, "local-model");
        // Global value preserved for analyzer_model
        assert_eq!(loaded.config.subagents.analyzer_model, "global-analyzer");
    }

    #[test]
    #[serial]
    fn test_env_overrides_files() {
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"{"reasoning": {"optimizer_model": "file-model"}}"#,
        )
        .unwrap();

        // Set env var
        // SAFETY: This test runs serially via #[serial] to avoid data races
        unsafe {
            std::env::set_var("AGENTIC_REASONING_OPTIMIZER_MODEL", "env-model");
        }

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(loaded.config.reasoning.optimizer_model, "env-model");

        // Clean up
        // SAFETY: This test runs serially via #[serial] to avoid data races
        unsafe {
            std::env::remove_var("AGENTIC_REASONING_OPTIMIZER_MODEL");
        }
    }

    #[test]
    #[serial]
    fn test_env_trimmed_ignores_whitespace() {
        // SAFETY: This test runs serially via #[serial] to avoid data races
        unsafe {
            std::env::set_var("TEST_AGENTIC_TRIM", "  value  ");
        }
        let result = env_trimmed("TEST_AGENTIC_TRIM");
        assert_eq!(result, Some("value".to_string()));

        // SAFETY: This test runs serially via #[serial] to avoid data races
        unsafe {
            std::env::set_var("TEST_AGENTIC_EMPTY", "   ");
        }
        let result = env_trimmed("TEST_AGENTIC_EMPTY");
        assert_eq!(result, None);

        // SAFETY: This test runs serially via #[serial] to avoid data races
        unsafe {
            std::env::remove_var("TEST_AGENTIC_TRIM");
            std::env::remove_var("TEST_AGENTIC_EMPTY");
        }
    }

    #[test]
    fn test_invalid_json_errors() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(LOCAL_FILE), "not valid json").unwrap();

        let result = load_merged(temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_non_object_root_errors() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join(LOCAL_FILE), "[1, 2, 3]").unwrap();

        let result = load_merged(temp.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must be a JSON object")
        );
    }

    #[test]
    #[serial]
    fn test_local_value_overrides_struct_default() {
        // Test that local config values override struct defaults (not RFC 7396 null deletion,
        // which is tested in merge.rs). This verifies the merge behavior for nested objects.

        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        // Create local config that overrides a nested value
        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"{"thoughts": {"mount_dirs": {"thoughts": "custom"}}}"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(loaded.config.thoughts.mount_dirs.thoughts, "custom");
    }

    #[test]
    #[serial]
    fn test_paths_are_set() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        let loaded = load_merged(temp.path()).unwrap();

        assert_eq!(loaded.paths.local, temp.path().join(LOCAL_FILE));
        // Global path should point to our isolated temp dir
        assert_eq!(
            loaded.paths.global,
            temp.path().join(GLOBAL_DIR).join(GLOBAL_FILE)
        );
    }

    #[test]
    #[serial]
    fn test_load_legacy_only_is_read_only_and_uses_legacy_values() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        // Create legacy config
        let thoughts_dir = temp.path().join(".thoughts");
        std::fs::create_dir_all(&thoughts_dir).unwrap();
        std::fs::write(
            thoughts_dir.join("config.json"),
            r#"{
                "version": "2.0",
                "mount_dirs": {
                    "thoughts": "legacy-thoughts",
                    "context": "legacy-context",
                    "references": "legacy-refs"
                }
            }"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();

        // Legacy values are applied
        assert_eq!(
            loaded.config.thoughts.mount_dirs.thoughts,
            "legacy-thoughts"
        );

        // Read-only: loader must not create agentic.json
        assert!(!temp.path().join(LOCAL_FILE).exists());

        // Warning must point to manual migration path
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.code == "legacy_config.used"
                    && w.message.contains("agentic config show > agentic.json"))
        );
    }

    #[test]
    #[serial]
    fn test_agentic_json_wins_over_legacy_and_no_legacy_warning() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        // Create legacy config
        let thoughts_dir = temp.path().join(".thoughts");
        std::fs::create_dir_all(&thoughts_dir).unwrap();
        std::fs::write(
            thoughts_dir.join("config.json"),
            r#"{"version":"2.0","mount_dirs":{"thoughts":"legacy-thoughts"}}"#,
        )
        .unwrap();

        // Create local agentic.json that should win
        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"{"thoughts":{"mount_dirs":{"thoughts":"agentic-thoughts"}}}"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(
            loaded.config.thoughts.mount_dirs.thoughts,
            "agentic-thoughts"
        );

        // Legacy warning should not appear because legacy was not used
        assert!(
            !loaded
                .warnings
                .iter()
                .any(|w| w.code == "legacy_config.used")
        );
    }

    #[test]
    fn test_invalid_legacy_json_errors() {
        let temp = TempDir::new().unwrap();
        let thoughts_dir = temp.path().join(".thoughts");
        std::fs::create_dir_all(&thoughts_dir).unwrap();
        std::fs::write(thoughts_dir.join("config.json"), "not valid json").unwrap();

        let result = load_merged(temp.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid JSON in legacy config")
        );
    }

    #[test]
    fn test_legacy_v1_errors() {
        let temp = TempDir::new().unwrap();
        let thoughts_dir = temp.path().join(".thoughts");
        std::fs::create_dir_all(&thoughts_dir).unwrap();
        std::fs::write(thoughts_dir.join("config.json"), r#"{"version":"1.0"}"#).unwrap();

        let result = load_merged(temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("only v2"));
    }
}
