//! Config types for opencode_rs.

use serde::{Deserialize, Serialize};

/// OpenCode configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Default provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Default model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Default agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Auto-compaction settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact: Option<AutoCompactConfig>,
    /// MCP servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<serde_json::Value>,
    /// Additional configuration.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Auto-compaction configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoCompactConfig {
    /// Whether auto-compaction is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Token threshold for triggering compaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u64>,
}

/// Request to update configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigRequest {
    /// New default provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// New default model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// New default agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Additional configuration updates.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Provider configuration info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProviders {
    /// List of available providers.
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

/// Provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Provider identifier.
    pub id: String,
    /// Provider display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the provider is configured.
    #[serde(default)]
    pub configured: bool,
    /// Auth method type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
}
