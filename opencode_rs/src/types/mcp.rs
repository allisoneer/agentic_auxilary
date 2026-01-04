//! MCP (Model Context Protocol) types for opencode_rs.

use serde::{Deserialize, Serialize};

/// MCP server status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    /// List of configured MCP servers.
    #[serde(default)]
    pub servers: Vec<McpServer>,
}

/// An MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    /// Server name.
    pub name: String,
    /// Server command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Server arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<serde_json::Value>,
    /// Connection status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<McpConnectionStatus>,
    /// Available tools from this server.
    #[serde(default)]
    pub tools: Vec<McpTool>,
    /// Error message if connection failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// MCP connection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpConnectionStatus {
    /// Not connected.
    Disconnected,
    /// Connecting.
    Connecting,
    /// Connected.
    Connected,
    /// Connection failed.
    Error,
    /// Unknown status (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// An MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    /// Tool name.
    pub name: String,
    /// Tool description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Input schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

/// Request to add an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAddRequest {
    /// Server name.
    pub name: String,
    /// Server command.
    pub command: String,
    /// Server arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<serde_json::Value>,
}

/// MCP auth start request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuthStartRequest {
    /// Callback URL for OAuth flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
}

/// MCP auth start response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuthStartResponse {
    /// Authorization URL.
    pub url: String,
}

/// MCP auth callback request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuthCallbackRequest {
    /// Authorization code.
    pub code: String,
    /// State parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// MCP authenticate request (for API key auth).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuthenticateRequest {
    /// The API key or token.
    pub token: String,
}
