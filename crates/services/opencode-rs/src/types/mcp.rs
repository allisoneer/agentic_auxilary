//! MCP (Model Context Protocol) types for opencode_rs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub env: Option<HashMap<String, String>>,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum McpConnectionStatus {
    /// Connected and operational.
    Connected,
    /// Not connected.
    Disconnected,
    /// Connecting in progress.
    Connecting,
    /// Connection failed with error.
    Error,
    /// Connection is disabled.
    Disabled,
    /// Needs authentication.
    NeedsAuth,
    /// Needs client registration.
    NeedsClientRegistration,
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
    pub env: Option<HashMap<String, String>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_connection_status_serialize() {
        assert_eq!(
            serde_json::to_string(&McpConnectionStatus::Connected).unwrap(),
            r#""connected""#
        );
        assert_eq!(
            serde_json::to_string(&McpConnectionStatus::Disconnected).unwrap(),
            r#""disconnected""#
        );
        assert_eq!(
            serde_json::to_string(&McpConnectionStatus::Connecting).unwrap(),
            r#""connecting""#
        );
        assert_eq!(
            serde_json::to_string(&McpConnectionStatus::Error).unwrap(),
            r#""error""#
        );
        assert_eq!(
            serde_json::to_string(&McpConnectionStatus::Disabled).unwrap(),
            r#""disabled""#
        );
        assert_eq!(
            serde_json::to_string(&McpConnectionStatus::NeedsAuth).unwrap(),
            r#""needs_auth""#
        );
        assert_eq!(
            serde_json::to_string(&McpConnectionStatus::NeedsClientRegistration).unwrap(),
            r#""needs_client_registration""#
        );
    }

    #[test]
    fn test_mcp_connection_status_deserialize() {
        assert_eq!(
            serde_json::from_str::<McpConnectionStatus>(r#""connected""#).unwrap(),
            McpConnectionStatus::Connected
        );
        assert_eq!(
            serde_json::from_str::<McpConnectionStatus>(r#""disconnected""#).unwrap(),
            McpConnectionStatus::Disconnected
        );
        assert_eq!(
            serde_json::from_str::<McpConnectionStatus>(r#""needs_auth""#).unwrap(),
            McpConnectionStatus::NeedsAuth
        );
        assert_eq!(
            serde_json::from_str::<McpConnectionStatus>(r#""needs_client_registration""#).unwrap(),
            McpConnectionStatus::NeedsClientRegistration
        );
    }

    #[test]
    fn test_mcp_connection_status_unknown() {
        // Unknown status should deserialize as Unknown
        assert_eq!(
            serde_json::from_str::<McpConnectionStatus>(r#""some_future_status""#).unwrap(),
            McpConnectionStatus::Unknown
        );
    }

    #[test]
    fn test_mcp_server_minimal() {
        let json = r#"{"name": "my-server"}"#;
        let server: McpServer = serde_json::from_str(json).unwrap();
        assert_eq!(server.name, "my-server");
        assert!(server.command.is_none());
        assert!(server.args.is_empty());
        assert!(server.status.is_none());
    }

    #[test]
    fn test_mcp_server_with_status() {
        let json = r#"{
            "name": "my-server",
            "command": "npx",
            "args": ["-y", "@my/server"],
            "status": "connected"
        }"#;
        let server: McpServer = serde_json::from_str(json).unwrap();
        assert_eq!(server.name, "my-server");
        assert_eq!(server.command, Some("npx".to_string()));
        assert_eq!(server.args, vec!["-y", "@my/server"]);
        assert_eq!(server.status, Some(McpConnectionStatus::Connected));
    }

    #[test]
    fn test_mcp_status() {
        let json = r#"{"servers": [{"name": "server1"}, {"name": "server2"}]}"#;
        let status: McpStatus = serde_json::from_str(json).unwrap();
        assert_eq!(status.servers.len(), 2);
        assert_eq!(status.servers[0].name, "server1");
        assert_eq!(status.servers[1].name, "server2");
    }

    #[test]
    fn test_mcp_add_request() {
        let json = r#"{
            "name": "my-server",
            "command": "node",
            "args": ["server.js"],
            "env": {"NODE_ENV": "production"}
        }"#;
        let req: McpAddRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "my-server");
        assert_eq!(req.command, "node");
        assert_eq!(req.args, vec!["server.js"]);
        let env = req.env.unwrap();
        assert_eq!(env.get("NODE_ENV"), Some(&"production".to_string()));
    }
}
