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
pub fn global_config_path() -> Result<PathBuf> {
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
/// # Migration
/// If `./agentic.json` doesn't exist but `./.thoughts/config.json` does (V2 format),
/// migration will be attempted automatically. The legacy file remains untouched.
pub fn load_merged(local_dir: &Path) -> Result<LoadedAgenticConfig> {
    let global_path = global_config_path()?;
    let local_path = local_config_path(local_dir);

    let mut events = vec![];

    // Check for migration opportunity
    if let Some(legacy_path) = crate::migration::should_migrate(local_dir, &local_path) {
        match attempt_migration(&legacy_path, &local_path) {
            Ok(event) => events.push(event),
            Err(e) => {
                // Log migration failure but continue - we can still load defaults
                tracing::warn!("Migration from legacy config failed: {}", e);
            }
        }
    }

    // Read configs as JSON Values
    let global_v = read_json_object_or_empty(&global_path)?;
    let local_v = read_json_object_or_empty(&local_path)?;

    // Merge: global as base, local as patch
    let merged = merge_patch(global_v, local_v);

    // Deserialize to typed config
    let mut cfg: AgenticConfig =
        serde_json::from_value(merged).context("Failed to deserialize merged agentic config")?;

    // Apply env var overrides (highest precedence)
    apply_env_overrides(&mut cfg);

    // Run advisory validation
    let warnings = crate::validation::validate(&cfg);

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

    // Model overrides
    if let Some(v) = env_trimmed("AGENTIC_MODEL_DEFAULT") {
        cfg.models.default_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_MODEL_REASONING") {
        cfg.models.reasoning_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_MODEL_FAST") {
        cfg.models.fast_model = v;
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

/// Attempt to migrate legacy V2 config to agentic.json.
fn attempt_migration(legacy_path: &Path, local_path: &Path) -> Result<LoadEvent> {
    let v2 = crate::migration::read_legacy_v2(legacy_path)?;
    let agentic_value = crate::migration::map_v2_to_agentic_value(v2)?;
    crate::writer::write_pretty_json_atomic(local_path, &agentic_value)?;

    Ok(LoadEvent::MigratedThoughtsV2 {
        from: legacy_path.to_path_buf(),
        to: local_path.to_path_buf(),
    })
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
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn test_load_no_files_returns_defaults() {
        let temp = TempDir::new().unwrap();
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
    fn test_load_local_only() {
        let temp = TempDir::new().unwrap();
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
    fn test_local_overrides_global() {
        let temp = TempDir::new().unwrap();

        // Create a fake global config dir
        let global_dir = temp.path().join("global_config").join(GLOBAL_DIR);
        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::write(
            global_dir.join(GLOBAL_FILE),
            r#"{"models": {"default_model": "global-model"}}"#,
        )
        .unwrap();

        // Create local config
        let local_dir = temp.path().join("local_repo");
        std::fs::create_dir_all(&local_dir).unwrap();
        std::fs::write(
            local_dir.join(LOCAL_FILE),
            r#"{"models": {"default_model": "local-model"}}"#,
        )
        .unwrap();

        // Note: This test can't easily override the global path without mocking
        // We test the merge logic more directly in merge.rs tests
        let loaded = load_merged(&local_dir).unwrap();
        assert_eq!(loaded.config.models.default_model, "local-model");
    }

    #[test]
    #[serial]
    fn test_env_overrides_files() {
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"{"models": {"default_model": "file-model"}}"#,
        )
        .unwrap();

        // Set env var
        // SAFETY: This test runs serially via #[serial] to avoid data races
        unsafe {
            std::env::set_var("AGENTIC_MODEL_DEFAULT", "env-model");
        }

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(loaded.config.models.default_model, "env-model");

        // Clean up
        // SAFETY: This test runs serially via #[serial] to avoid data races
        unsafe {
            std::env::remove_var("AGENTIC_MODEL_DEFAULT");
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
    fn test_local_value_overrides_struct_default() {
        // Test that local config values override struct defaults (not RFC 7396 null deletion,
        // which is tested in merge.rs). This verifies the merge behavior for nested objects.

        let temp = TempDir::new().unwrap();
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
    fn test_paths_are_set() {
        let temp = TempDir::new().unwrap();
        let loaded = load_merged(temp.path()).unwrap();

        assert_eq!(loaded.paths.local, temp.path().join(LOCAL_FILE));
        assert!(loaded.paths.global.ends_with("agentic/agentic.json"));
    }
}
