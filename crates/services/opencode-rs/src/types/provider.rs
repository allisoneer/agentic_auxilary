//! Provider types for `opencode_rs`.

use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

/// Response from the provider list endpoint.
///
/// Contains all available providers along with defaults and connection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListResponse {
    /// All available providers.
    #[serde(default)]
    pub all: Vec<Provider>,
    /// Default model for each provider (`provider_id` -> `model_id`).
    #[serde(default)]
    pub default: HashMap<String, String>,
    /// List of connected/authenticated provider IDs.
    #[serde(default)]
    pub connected: Vec<String>,
}

/// Provider source type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum ProviderSource {
    /// From environment variable.
    Env,
    /// From config file.
    Config,
    /// Custom provider.
    Custom,
    /// From API.
    Api,
    /// Unknown source (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// A provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    /// Provider identifier.
    pub id: String,
    /// Provider display name.
    pub name: String,
    /// Source of this provider configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ProviderSource>,
    /// Environment variable names for this provider.
    #[serde(default)]
    pub env: Vec<String>,
    /// API key if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Provider options.
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    /// Available models for this provider (keyed by model ID).
    #[serde(default)]
    pub models: HashMap<String, Model>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// A model available from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// Model identifier.
    pub id: String,
    /// Model display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Model API configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<ModelApi>,
    /// Model capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ModelCapabilities>,
    /// Model cost information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<ModelCost>,
    /// Model limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<ModelLimit>,
    /// Model status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ModelStatus>,
    /// Custom headers for this model.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// Model release date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    /// Model variants.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variants: HashMap<String, serde_json::Value>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Model API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelApi {
    /// API endpoint override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Model cost information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    /// Cost per input token (in USD per million tokens).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,
    /// Cost per output token (in USD per million tokens).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<f64>,
    /// Cost per cache read token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    /// Cost per cache write token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Model limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelLimit {
    /// Maximum context window size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    /// Maximum output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Model status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    /// Model is available.
    Available,
    /// Model is deprecated.
    Deprecated,
    /// Model is in beta.
    Beta,
    /// Model is in preview.
    Preview,
    /// Unknown status (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// Model capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    /// Whether the model supports tool use.
    #[serde(default)]
    pub tool_use: bool,
    /// Whether the model supports vision/images.
    #[serde(default)]
    pub vision: bool,
    /// Whether the model supports extended thinking.
    #[serde(default)]
    pub thinking: bool,
    /// Interleaved reasoning support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<InterleavedCapability>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Interleaved reasoning capability (can be bool or config object).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InterleavedCapability {
    /// Simple boolean flag.
    Bool(bool),
    /// Detailed configuration.
    Config(InterleavedConfig),
    /// Unknown configuration (forward compatibility).
    Unknown(serde_json::Value),
}

/// Interleaved reasoning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterleavedConfig {
    /// The field to use for interleaved content.
    pub field: InterleavedField,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Field for interleaved reasoning content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum InterleavedField {
    /// Use `reasoning_content` field.
    ReasoningContent,
    /// Use `reasoning_details` field.
    ReasoningDetails,
    /// Unknown field type (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// Provider authentication method information.
pub type ProviderAuthMethod = AuthMethod;

/// Authentication method for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AuthMethod {
    /// API key authentication.
    ApiKey {
        /// Environment variable name for the key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        env_var: Option<String>,
    },
    /// OAuth authentication.
    Oauth {
        /// OAuth authorize URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authorize_url: Option<String>,
    },
    /// No authentication required.
    None,
    /// Unknown auth method (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// Request to set authentication for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAuthRequest {
    /// The API key or token.
    pub key: String,
}

/// OAuth authorize response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthAuthorizeResponse {
    /// The authorization URL to redirect to.
    pub url: String,
    /// Selected auth method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

/// OAuth authorize request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthAuthorizeRequest {
    /// Provider auth method to use.
    pub method: String,
    /// Optional method-specific inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
}

/// OAuth callback request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCallbackRequest {
    /// Provider auth method to use.
    pub method: String,
    /// The authorization code.
    pub code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_minimal() {
        let json = r#"{"id": "claude-3"}"#;
        let model: Model = serde_json::from_str(json).unwrap();
        assert_eq!(model.id, "claude-3");
        assert!(model.name.is_none());
        assert!(model.capabilities.is_none());
    }

    #[test]
    fn test_model_full() {
        let json = r#"{
            "id": "claude-3",
            "name": "Claude 3",
            "api": {"endpoint": "https://api.example.com"},
            "capabilities": {"toolUse": true, "vision": true, "thinking": false},
            "cost": {"input": 3.0, "output": 15.0},
            "limit": {"context": 200000, "output": 4096},
            "status": "available",
            "releaseDate": "2024-01-01",
            "headers": {"X-Custom": "value"}
        }"#;
        let model: Model = serde_json::from_str(json).unwrap();
        assert_eq!(model.id, "claude-3");
        assert_eq!(model.name, Some("Claude 3".to_string()));
        assert!(model.api.is_some());
        assert!(model.capabilities.is_some());
        let caps = model.capabilities.unwrap();
        assert!(caps.tool_use);
        assert!(caps.vision);
        assert!(!caps.thinking);
        assert!(model.cost.is_some());
        let cost = model.cost.unwrap();
        assert_eq!(cost.input, Some(3.0));
        assert_eq!(cost.output, Some(15.0));
        assert!(model.limit.is_some());
        assert_eq!(model.limit.unwrap().context, Some(200_000));
        assert_eq!(model.status, Some(ModelStatus::Available));
        assert_eq!(model.release_date, Some("2024-01-01".to_string()));
        assert_eq!(model.headers.get("X-Custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_model_status_variants() {
        assert_eq!(
            serde_json::from_str::<ModelStatus>(r#""available""#).unwrap(),
            ModelStatus::Available
        );
        assert_eq!(
            serde_json::from_str::<ModelStatus>(r#""deprecated""#).unwrap(),
            ModelStatus::Deprecated
        );
        assert_eq!(
            serde_json::from_str::<ModelStatus>(r#""beta""#).unwrap(),
            ModelStatus::Beta
        );
        assert_eq!(
            serde_json::from_str::<ModelStatus>(r#""preview""#).unwrap(),
            ModelStatus::Preview
        );
        assert_eq!(
            serde_json::from_str::<ModelStatus>(r#""future-status""#).unwrap(),
            ModelStatus::Unknown
        );
    }

    #[test]
    fn test_interleaved_capability_bool() {
        let json = r"true";
        let cap: InterleavedCapability = serde_json::from_str(json).unwrap();
        assert!(matches!(cap, InterleavedCapability::Bool(true)));
    }

    #[test]
    fn test_interleaved_capability_config() {
        let json = r#"{"field": "reasoning_content"}"#;
        let cap: InterleavedCapability = serde_json::from_str(json).unwrap();
        if let InterleavedCapability::Config(config) = cap {
            assert_eq!(config.field, InterleavedField::ReasoningContent);
        } else {
            panic!("Expected InterleavedCapability::Config");
        }
    }

    #[test]
    fn test_interleaved_capability_unknown_config() {
        let json = r#"{"field": "future_field"}"#;
        let cap: InterleavedCapability = serde_json::from_str(json).unwrap();
        if let InterleavedCapability::Config(config) = cap {
            assert_eq!(config.field, InterleavedField::Unknown);
        } else {
            panic!("Expected InterleavedCapability::Config");
        }
    }

    #[test]
    fn test_model_capabilities_with_interleaved() {
        let json = r#"{
            "toolUse": true,
            "vision": false,
            "thinking": true,
            "interleaved": {"field": "reasoning_details"}
        }"#;
        let caps: ModelCapabilities = serde_json::from_str(json).unwrap();
        assert!(caps.tool_use);
        assert!(!caps.vision);
        assert!(caps.thinking);
        assert!(caps.interleaved.is_some());
        if let Some(InterleavedCapability::Config(config)) = caps.interleaved {
            assert_eq!(config.field, InterleavedField::ReasoningDetails);
        } else {
            panic!("Expected InterleavedCapability::Config");
        }
    }

    #[test]
    fn test_provider_source_variants() {
        assert_eq!(
            serde_json::from_str::<ProviderSource>(r#""env""#).unwrap(),
            ProviderSource::Env
        );
        assert_eq!(
            serde_json::from_str::<ProviderSource>(r#""config""#).unwrap(),
            ProviderSource::Config
        );
        assert_eq!(
            serde_json::from_str::<ProviderSource>(r#""future""#).unwrap(),
            ProviderSource::Unknown
        );
    }
}
