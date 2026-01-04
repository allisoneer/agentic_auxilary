//! Provider types for opencode_rs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Provider source type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// Model capabilities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ModelCapabilities>,
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
}

/// Provider authentication info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuth {
    /// Provider identifier.
    pub provider_id: String,
    /// Authentication method.
    pub method: AuthMethod,
    /// Whether auth is configured.
    #[serde(default)]
    pub configured: bool,
}

/// Authentication method for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// OAuth callback request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCallbackRequest {
    /// The authorization code.
    pub code: String,
    /// Optional state parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}
