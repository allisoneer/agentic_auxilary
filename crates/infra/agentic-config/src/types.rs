//! Configuration types for the agentic tools ecosystem.
//!
//! The root type is [`AgenticConfig`], which contains namespaced sub-configs
//! for different concerns: thoughts workspace, external services, model selection,
//! and logging.

use schemars::JsonSchema;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

/// Root configuration for all agentic tools.
///
/// This is the unified configuration that gets loaded from `agentic.json` files.
/// All fields use `#[serde(default)]` so partial configs work correctly.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AgenticConfig {
    /// Optional JSON Schema URL for IDE autocomplete support.
    /// When present, editors can validate and provide completions.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Thoughts workspace configuration (directories, mounts, references).
    pub thoughts: ThoughtsConfig,

    /// External service configurations (Anthropic, Exa, etc.).
    pub services: ServicesConfig,

    /// Model selection and defaults.
    pub models: ModelsConfig,

    /// Logging and diagnostics configuration.
    pub logging: LoggingConfig,
}

/// Configuration for the thoughts workspace system.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ThoughtsConfig {
    /// Directory names within the repo for each mount type.
    pub mount_dirs: ThoughtsMountDirs,

    /// Optional thoughts repository mount (personal workspace).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_mount: Option<ThoughtsMount>,

    /// Context mounts (team-shared documentation repositories).
    pub context_mounts: Vec<ContextMount>,

    /// Reference repositories (read-only external code).
    pub references: Vec<ReferenceEntry>,
}

/// Directory names for the three-space architecture.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ThoughtsMountDirs {
    /// Directory for personal thoughts workspace.
    pub thoughts: String,

    /// Directory for team-shared context.
    pub context: String,

    /// Directory for read-only references.
    pub references: String,
}

impl Default for ThoughtsMountDirs {
    fn default() -> Self {
        Self {
            thoughts: "thoughts".into(),
            context: "context".into(),
            references: "references".into(),
        }
    }
}

/// Personal thoughts repository mount configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThoughtsMount {
    /// Git remote URL for the thoughts repository.
    pub remote: String,

    /// Optional subpath within the repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,

    /// Sync strategy for this mount.
    #[serde(default)]
    pub sync: SyncStrategy,
}

/// Context mount configuration (team-shared documentation).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextMount {
    /// Git remote URL.
    pub remote: String,

    /// Optional subpath within the repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,

    /// Mount path (directory name under context/).
    pub mount_path: String,

    /// Sync strategy for this mount.
    #[serde(default)]
    pub sync: SyncStrategy,
}

/// Reference entry - either a simple URL string or with metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ReferenceEntry {
    /// Simple URL string.
    Simple(String),

    /// URL with optional metadata.
    WithMetadata(ReferenceMount),
}

/// Reference mount with optional metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReferenceMount {
    /// Git remote URL.
    pub remote: String,

    /// Optional description of the reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Sync strategy for git-backed mounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncStrategy {
    /// No automatic syncing.
    #[default]
    None,

    /// Automatic sync on access.
    Auto,
}

/// External service configurations.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ServicesConfig {
    /// Anthropic API configuration.
    pub anthropic: AnthropicServiceConfig,

    /// Exa search API configuration.
    pub exa: ExaServiceConfig,
}

/// Anthropic API service configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AnthropicServiceConfig {
    /// Base URL for the Anthropic API.
    pub base_url: String,

    /// API key (env-only, never serialized to config files).
    #[serde(skip)]
    #[schemars(skip)]
    pub api_key: Option<SecretString>,
}

impl Default for AnthropicServiceConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com".into(),
            api_key: None,
        }
    }
}

/// Exa search API service configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ExaServiceConfig {
    /// Base URL for the Exa API.
    pub base_url: String,

    /// API key (env-only, never serialized to config files).
    #[serde(skip)]
    #[schemars(skip)]
    pub api_key: Option<SecretString>,
}

impl Default for ExaServiceConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.exa.ai".into(),
            api_key: None,
        }
    }
}

/// Model selection and defaults.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ModelsConfig {
    /// Default model to use for general tasks.
    pub default_model: String,

    /// Model to use for reasoning/planning tasks.
    pub reasoning_model: String,

    /// Model to use for fast/cheap tasks.
    pub fast_model: String,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            default_model: "claude-sonnet-4-20250514".into(),
            reasoning_model: "claude-sonnet-4-20250514".into(),
            fast_model: "claude-haiku-4-20250514".into(),
        }
    }
}

/// Logging and diagnostics configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error).
    pub level: String,

    /// Whether to enable JSON-formatted logs.
    pub json: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
            json: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serializes() {
        let config = AgenticConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("\"thoughts\""));
        assert!(json.contains("\"services\""));
        assert!(json.contains("\"models\""));
        assert!(json.contains("\"logging\""));
    }

    #[test]
    fn test_partial_config_deserializes() {
        let json = r#"{"thoughts": {"mount_dirs": {"thoughts": "my-thoughts"}}}"#;
        let config: AgenticConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.thoughts.mount_dirs.thoughts, "my-thoughts");
        // Other fields get defaults
        assert_eq!(config.thoughts.mount_dirs.context, "context");
        assert_eq!(
            config.services.anthropic.base_url,
            "https://api.anthropic.com"
        );
    }

    #[test]
    fn test_schema_field_optional() {
        let json = r#"{"$schema": "file://./agentic.schema.json"}"#;
        let config: AgenticConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.schema, Some("file://./agentic.schema.json".into()));
    }

    #[test]
    fn test_reference_entry_simple() {
        let json = r#""git@github.com:org/repo.git""#;
        let entry: ReferenceEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, ReferenceEntry::Simple(_)));
    }

    #[test]
    fn test_reference_entry_with_metadata() {
        let json = r#"{"remote": "https://github.com/org/repo", "description": "Test"}"#;
        let entry: ReferenceEntry = serde_json::from_str(json).unwrap();
        match entry {
            ReferenceEntry::WithMetadata(rm) => {
                assert_eq!(rm.remote, "https://github.com/org/repo");
                assert_eq!(rm.description.as_deref(), Some("Test"));
            }
            _ => panic!("Expected WithMetadata"),
        }
    }

    #[test]
    fn test_sync_strategy_default() {
        assert_eq!(SyncStrategy::default(), SyncStrategy::None);
    }

    #[test]
    fn test_api_keys_not_serialized() {
        let mut config = AgenticConfig::default();
        config.services.anthropic.api_key = Some(SecretString::from("secret-key".to_string()));

        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("secret-key"));
        assert!(!json.contains("api_key"));
    }
}
