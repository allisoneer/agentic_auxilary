//! Config types for opencode_rs.

use serde::{Deserialize, Serialize};

/// OpenCode configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Default provider (can be string or object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<serde_json::Value>,
    /// Default model (can be string or object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<serde_json::Value>,
    /// Default agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<serde_json::Value>,
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

/// Lightweight config info with commonly-used typed fields.
///
/// This provides typed access to frequently-used config fields while
/// preserving unknown fields via flatten. Use this when you need quick
/// access to common config values without full Config.Info typing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInfoLite {
    /// Default model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Small model for lightweight tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_model: Option<String>,
    /// Default agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    /// Share setting ("manual" | "auto" | "disabled").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
    /// MCP configuration (complex type, kept as Value).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<serde_json::Value>,
    /// Additional config fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_info_lite_deserialize() {
        let json = r#"{
            "model": "claude-3-opus",
            "smallModel": "claude-3-haiku",
            "defaultAgent": "code",
            "share": "manual",
            "otherField": "preserved"
        }"#;
        let config: ConfigInfoLite = serde_json::from_str(json).unwrap();
        assert_eq!(config.model, Some("claude-3-opus".to_string()));
        assert_eq!(config.small_model, Some("claude-3-haiku".to_string()));
        assert_eq!(config.default_agent, Some("code".to_string()));
        assert_eq!(config.share, Some("manual".to_string()));
        assert_eq!(config.extra.get("otherField").unwrap(), "preserved");
    }

    #[test]
    fn test_config_info_lite_minimal() {
        let json = r#"{}"#;
        let config: ConfigInfoLite = serde_json::from_str(json).unwrap();
        assert!(config.model.is_none());
        assert!(config.small_model.is_none());
    }
}
