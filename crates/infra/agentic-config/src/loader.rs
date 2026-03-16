//! Configuration loader with two-layer merge and env overrides.
//!
//! The loading process:
//! 1. Read global config from `~/.config/agentic/agentic.toml`
//! 2. Read local config from `./agentic.toml`
//! 3. Deep merge at TOML Value level (tables merge, arrays/scalars replace)
//! 4. Deserialize once into typed AgenticConfig
//! 5. Apply env var overrides (highest precedence)
//! 6. Run advisory validation

use crate::{merge::deep_merge, types::AgenticConfig, validation::AdvisoryWarning};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Filename for local config (TOML format).
pub const LOCAL_FILE: &str = "agentic.toml";

/// Directory name under config_dir for global config.
pub const GLOBAL_DIR: &str = "agentic";

/// Filename for global config (TOML format).
pub const GLOBAL_FILE: &str = "agentic.toml";

/// Legacy JSON filename (warn if found).
const LEGACY_JSON_FILE: &str = "agentic.json";

/// Known top-level keys for unknown key detection.
/// Unknown keys at root level produce advisory warnings.
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &[
    "$schema",
    "subagents",
    "reasoning",
    "services",
    "orchestrator",
    "web_retrieval",
    "cli_tools",
    "logging",
];

/// Resolved paths for config files.
#[derive(Debug, Clone)]
pub struct AgenticConfigPaths {
    /// Path to local config (./agentic.toml).
    pub local: PathBuf,

    /// Path to global config (~/.config/agentic/agentic.toml).
    pub global: PathBuf,
}

/// Result of loading configuration.
#[derive(Debug)]
pub struct LoadedAgenticConfig {
    /// The loaded and merged configuration.
    pub config: AgenticConfig,

    /// Advisory warnings from validation.
    pub warnings: Vec<AdvisoryWarning>,

    /// Resolved config file paths.
    pub paths: AgenticConfigPaths,
}

/// Get the global config file path.
///
/// Returns `~/.config/agentic/agentic.toml` on Unix-like systems.
///
/// # Test Hook
///
/// Set `__AGENTIC_CONFIG_DIR_FOR_TESTS` to override the base config directory.
/// This is handled by [`crate::paths::xdg_config_home()`] which implements XDG
/// path resolution with test hook support.
pub fn global_config_path() -> Result<PathBuf> {
    let base = crate::paths::xdg_config_home()?;
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
/// 2. Global config (`~/.config/agentic/agentic.toml`)
/// 3. Local config (`./agentic.toml`)
/// 4. Environment variables
pub fn load_merged(local_dir: &Path) -> Result<LoadedAgenticConfig> {
    let global_path = global_config_path()?;
    let local_path = local_config_path(local_dir);

    // Collect warnings
    let mut warnings = Vec::new();

    // Warn if legacy agentic.json exists (it's now ignored)
    warn_legacy_json_if_present(&local_path, &global_path, &mut warnings);

    // Read configs as TOML Values
    let global_v = read_toml_table_or_empty(&global_path)?;
    let local_v = read_toml_table_or_empty(&local_path)?;

    // Merge: global as base, local as patch
    let merged = deep_merge(global_v, local_v);

    // Detect unknown top-level keys
    warn_unknown_top_level_keys(&merged, &mut warnings);

    // Detect deprecated keys from merged config (before deserialization)
    warnings.extend(crate::validation::detect_deprecated_keys_toml(&merged));

    // Deserialize to typed config using serde_path_to_error for better error messages
    let cfg: AgenticConfig = {
        let deserializer = merged.clone();
        serde_path_to_error::deserialize(deserializer)
            .with_context(|| "Failed to deserialize merged agentic config")?
    };

    // Apply env var overrides (highest precedence)
    let mut cfg = cfg;
    apply_env_overrides(&mut cfg);

    // Run advisory validation and add to warnings
    warnings.extend(crate::validation::validate(&cfg));

    Ok(LoadedAgenticConfig {
        config: cfg,
        warnings,
        paths: AgenticConfigPaths {
            local: local_path,
            global: global_path,
        },
    })
}

/// Apply environment variable overrides to the config.
fn apply_env_overrides(cfg: &mut AgenticConfig) {
    // --- Service URLs ---
    if let Some(v) = env_trimmed("ANTHROPIC_BASE_URL") {
        cfg.services.anthropic.base_url = v;
    }
    if let Some(v) = env_trimmed("EXA_BASE_URL") {
        cfg.services.exa.base_url = v;
    }
    if let Some(v) = env_trimmed("OPENCODE_BASE_URL") {
        cfg.services.opencode.base_url = v;
    }
    if let Some(v) = env_trimmed("LINEAR_BASE_URL") {
        cfg.services.linear.base_url = v;
    }
    if let Some(v) = env_trimmed("GITHUB_BASE_URL") {
        cfg.services.github.base_url = v;
    }

    // --- API keys (env-only) ---
    if let Some(k) = env_trimmed("ANTHROPIC_API_KEY") {
        cfg.services.anthropic.api_key = Some(secrecy::SecretString::from(k));
    }
    if let Some(k) = env_trimmed("EXA_API_KEY") {
        cfg.services.exa.api_key = Some(secrecy::SecretString::from(k));
    }
    if let Some(k) = env_trimmed("OPENCODE_API_KEY") {
        cfg.services.opencode.api_key = Some(secrecy::SecretString::from(k));
    }
    if let Some(k) = env_trimmed("LINEAR_API_KEY") {
        cfg.services.linear.api_key = Some(secrecy::SecretString::from(k));
    }
    if let Some(k) = env_trimmed("GITHUB_TOKEN") {
        cfg.services.github.token = Some(secrecy::SecretString::from(k));
    }

    // --- Subagents model overrides ---
    if let Some(v) = env_trimmed("AGENTIC_SUBAGENTS_LOCATOR_MODEL") {
        cfg.subagents.locator_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_SUBAGENTS_ANALYZER_MODEL") {
        cfg.subagents.analyzer_model = v;
    }

    // --- Reasoning model overrides ---
    if let Some(v) = env_trimmed("AGENTIC_REASONING_OPTIMIZER_MODEL") {
        cfg.reasoning.optimizer_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_REASONING_EXECUTOR_MODEL") {
        cfg.reasoning.executor_model = v;
    }
    if let Some(v) = env_trimmed("AGENTIC_REASONING_EFFORT") {
        cfg.reasoning.reasoning_effort = Some(v);
    }
    if let Some(v) = env_trimmed("AGENTIC_REASONING_API_BASE_URL") {
        cfg.reasoning.api_base_url = Some(v);
    }
    if let Some(v) = env_trimmed("AGENTIC_REASONING_TOKEN_LIMIT")
        && let Ok(n) = v.parse()
    {
        cfg.reasoning.token_limit = Some(n);
    }

    // --- Logging overrides ---
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

/// Warn about unknown top-level keys in the merged config.
fn warn_unknown_top_level_keys(v: &toml::Value, warnings: &mut Vec<AdvisoryWarning>) {
    let Some(tbl) = v.as_table() else { return };
    for key in tbl.keys() {
        if !KNOWN_TOP_LEVEL_KEYS.contains(&key.as_str()) {
            warnings.push(AdvisoryWarning::new(
                "config.unknown_top_level_key",
                "$",
                format!("Unknown top-level key '{}' will be ignored", key),
            ));
        }
    }
}

/// Warn if legacy agentic.json files exist (they are now ignored).
fn warn_legacy_json_if_present(
    local_path: &Path,
    global_path: &Path,
    warnings: &mut Vec<AdvisoryWarning>,
) {
    // Check local directory for legacy JSON
    if let Some(parent) = local_path.parent() {
        let legacy_local = parent.join(LEGACY_JSON_FILE);
        if legacy_local.exists() {
            warnings.push(AdvisoryWarning::new(
                "config.legacy_json_ignored",
                "$",
                format!(
                    "Found legacy config {} (ignored). Use agentic.toml instead.",
                    legacy_local.display()
                ),
            ));
        }
    }

    // Check global directory for legacy JSON
    if let Some(parent) = global_path.parent() {
        let legacy_global = parent.join(LEGACY_JSON_FILE);
        if legacy_global.exists() {
            warnings.push(AdvisoryWarning::new(
                "config.legacy_json_ignored",
                "$",
                format!(
                    "Found legacy config {} (ignored). Use agentic.toml instead.",
                    legacy_global.display()
                ),
            ));
        }
    }
}

/// Read a TOML file as a Value, returning empty table if file doesn't exist.
fn read_toml_table_or_empty(path: &Path) -> Result<toml::Value> {
    if !path.exists() {
        return Ok(toml::Value::Table(Default::default()));
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;

    let v: toml::Value =
        toml::from_str(&raw).with_context(|| format!("Invalid TOML in {}", path.display()))?;

    match v {
        toml::Value::Table(_) => Ok(v),
        _ => anyhow::bail!("Config root must be a TOML table: {}", path.display()),
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
        assert_eq!(loaded.config.orchestrator.session_deadline_secs, 3600);
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
            r#"
[orchestrator]
session_deadline_secs = 7200
"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(loaded.config.orchestrator.session_deadline_secs, 7200);
        // Other fields should be defaults
        assert_eq!(loaded.config.orchestrator.inactivity_timeout_secs, 300);
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
            r#"
[subagents]
locator_model = "global-model"
analyzer_model = "global-analyzer"
"#,
        )
        .unwrap();

        // Create local config that overrides locator_model but not analyzer_model
        let local_dir = temp.path().join("local_repo");
        std::fs::create_dir_all(&local_dir).unwrap();
        std::fs::write(
            local_dir.join(LOCAL_FILE),
            r#"
[subagents]
locator_model = "local-model"
"#,
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
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());
        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"
[reasoning]
optimizer_model = "file-model"
"#,
        )
        .unwrap();

        // Set env var via EnvGuard (RAII cleanup)
        let _env_guard = EnvGuard::set("AGENTIC_REASONING_OPTIMIZER_MODEL", "env-model");

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(loaded.config.reasoning.optimizer_model, "env-model");
    }

    #[test]
    #[serial]
    fn test_env_overrides_new_services() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        let _env1 = EnvGuard::set("OPENCODE_BASE_URL", "http://localhost:9999");
        let _env2 = EnvGuard::set("LINEAR_BASE_URL", "https://custom.linear.app/graphql");
        let _env3 = EnvGuard::set("GITHUB_BASE_URL", "https://github.example.com/api");

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(
            loaded.config.services.opencode.base_url,
            "http://localhost:9999"
        );
        assert_eq!(
            loaded.config.services.linear.base_url,
            "https://custom.linear.app/graphql"
        );
        assert_eq!(
            loaded.config.services.github.base_url,
            "https://github.example.com/api"
        );
    }

    #[test]
    #[serial]
    fn test_env_trimmed_ignores_whitespace() {
        // Use EnvGuard for RAII cleanup
        let _g1 = EnvGuard::set("TEST_AGENTIC_TRIM", "  value  ");
        let result = env_trimmed("TEST_AGENTIC_TRIM");
        assert_eq!(result, Some("value".to_string()));

        let _g2 = EnvGuard::set("TEST_AGENTIC_EMPTY", "   ");
        let result = env_trimmed("TEST_AGENTIC_EMPTY");
        assert_eq!(result, None);
    }

    #[test]
    #[serial]
    fn test_invalid_toml_errors() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());
        std::fs::write(temp.path().join(LOCAL_FILE), "not valid toml [[[").unwrap();

        let result = load_merged(temp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid TOML"));
    }

    #[test]
    #[serial]
    fn test_local_value_overrides_struct_default() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        // Create local config that overrides a nested value
        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"
[web_retrieval]
request_timeout_secs = 60
"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert_eq!(loaded.config.web_retrieval.request_timeout_secs, 60);
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
    fn test_warns_on_unknown_top_level_key() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"
typo = 1
unknown_section = "value"
"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.code == "config.unknown_top_level_key" && w.message.contains("typo"))
        );
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.code == "config.unknown_top_level_key"
                    && w.message.contains("unknown_section"))
        );
    }

    #[test]
    #[serial]
    fn test_warns_on_deprecated_thoughts_section() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        std::fs::write(
            temp.path().join(LOCAL_FILE),
            r#"
[thoughts]
mount_dirs = {}
"#,
        )
        .unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.code == "config.deprecated.thoughts")
        );
        // Also warns as unknown key
        assert!(loaded
            .warnings
            .iter()
            .any(|w| w.code == "config.unknown_top_level_key" && w.message.contains("thoughts")));
    }

    #[test]
    #[serial]
    fn test_warns_on_legacy_json_local() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        // Create a legacy agentic.json file
        std::fs::write(temp.path().join("agentic.json"), "{}").unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.code == "config.legacy_json_ignored"
                    && w.message.contains("agentic.json"))
        );
    }

    #[test]
    #[serial]
    fn test_warns_on_legacy_json_global() {
        let temp = TempDir::new().unwrap();
        let _guard = EnvGuard::set(CONFIG_DIR_TEST_VAR, temp.path());

        // Create global dir with legacy agentic.json
        let global_dir = temp.path().join(GLOBAL_DIR);
        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::write(global_dir.join("agentic.json"), "{}").unwrap();

        let loaded = load_merged(temp.path()).unwrap();
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.code == "config.legacy_json_ignored"
                    && w.message.contains("agentic.json"))
        );
    }
}
