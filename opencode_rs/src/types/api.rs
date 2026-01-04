//! HTTP API response types for opencode_rs.
//!
//! Typed response wrappers for HTTP endpoints, replacing serde_json::Value returns.

use serde::{Deserialize, Serialize};

// ==================== Messages API Responses ====================

/// Response from prompt and prompt_async endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResponse {
    /// Status of the prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Message ID created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response from command endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    /// Status of the command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response from shell endpoint.
pub type ShellResponse = CommandResponse;

// ==================== Find API Responses ====================

/// Response from find text/files/symbols endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResponse {
    /// Search results (structure varies by endpoint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<serde_json::Value>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ==================== Provider API Responses ====================

/// Response from OAuth callback endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCallbackResponse {
    /// Whether the operation succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// Message from server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response from set auth endpoint.
pub type SetAuthResponse = OAuthCallbackResponse;

// ==================== MCP API Responses ====================

/// Response from MCP action endpoints (add, auth_callback, authenticate, connect, disconnect).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpActionResponse {
    /// Whether the operation succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// Whether connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
    /// Server name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ==================== Misc API Responses ====================

/// Status of an LSP server.
///
/// The `/lsp` endpoint returns an array of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspServerStatus {
    /// Server ID.
    pub id: String,
    /// Server name.
    pub name: String,
    /// Root directory path (relative to instance directory).
    pub root: String,
    /// Connection status.
    pub status: LspConnectionStatus,
}

/// LSP server connection status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LspConnectionStatus {
    /// Connected and running.
    Connected,
    /// Error state.
    Error,
    /// Unknown status (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// Status of a formatter.
///
/// The `/formatter` endpoint returns an array of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatterInfo {
    /// Formatter name.
    pub name: String,
    /// File extensions this formatter handles.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Whether formatter is enabled.
    #[serde(default)]
    pub enabled: bool,
}

/// Response from OpenAPI doc endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiDoc {
    /// The OpenAPI spec (full document).
    #[serde(flatten)]
    pub spec: serde_json::Value,
}

// ==================== Parts API Responses ====================

/// Response from part update endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePartResponse {
    /// Updated part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<crate::types::message::Part>,
    /// Delta text if streaming.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ==================== Permission API Responses ====================

/// Response from permission reply endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReplyResponse {
    /// Session ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Request ID that was replied to.
    pub request_id: String,
    /// The reply that was sent.
    pub reply: crate::types::permission::PermissionReply,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_response_deserialize() {
        let json = r#"{"status":"ok","messageId":"msg-123"}"#;
        let resp: PromptResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, Some("ok".to_string()));
        assert_eq!(resp.message_id, Some("msg-123".to_string()));
    }

    #[test]
    fn test_prompt_response_with_extra() {
        let json = r#"{"status":"ok","messageId":"msg-123","futureField":"value"}"#;
        let resp: PromptResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.extra.get("futureField").unwrap(), "value");
    }

    #[test]
    fn test_command_response_deserialize() {
        let json = r#"{"status":"executed"}"#;
        let resp: CommandResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, Some("executed".to_string()));
    }

    #[test]
    fn test_find_response_deserialize() {
        let json = r#"{"results":[{"file":"test.rs","line":10}]}"#;
        let resp: FindResponse = serde_json::from_str(json).unwrap();
        assert!(resp.results.is_some());
    }

    #[test]
    fn test_oauth_callback_response_deserialize() {
        let json = r#"{"ok":true,"message":"Authentication successful"}"#;
        let resp: OAuthCallbackResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.ok, Some(true));
        assert_eq!(resp.message, Some("Authentication successful".to_string()));
    }

    #[test]
    fn test_mcp_action_response_deserialize() {
        let json = r#"{"ok":true,"connected":true,"name":"my-server"}"#;
        let resp: McpActionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.ok, Some(true));
        assert_eq!(resp.connected, Some(true));
        assert_eq!(resp.name, Some("my-server".to_string()));
    }

    #[test]
    fn test_lsp_server_status_deserialize() {
        let json = r#"{"id":"ra-1","name":"rust-analyzer","root":"./","status":"connected"}"#;
        let resp: LspServerStatus = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "ra-1");
        assert_eq!(resp.name, "rust-analyzer");
        assert_eq!(resp.status, LspConnectionStatus::Connected);
    }

    #[test]
    fn test_lsp_server_status_array_deserialize() {
        let json = r#"[{"id":"ra-1","name":"rust-analyzer","root":"./","status":"connected"}]"#;
        let resp: Vec<LspServerStatus> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.len(), 1);
        assert_eq!(resp[0].name, "rust-analyzer");
    }

    #[test]
    fn test_formatter_info_deserialize() {
        let json = r#"{"name":"rustfmt","extensions":[".rs"],"enabled":true}"#;
        let resp: FormatterInfo = serde_json::from_str(json).unwrap();
        assert_eq!(resp.name, "rustfmt");
        assert!(resp.enabled);
        assert_eq!(resp.extensions, vec![".rs"]);
    }

    #[test]
    fn test_formatter_info_array_deserialize() {
        let json = r#"[{"name":"rustfmt","extensions":[".rs"],"enabled":true}]"#;
        let resp: Vec<FormatterInfo> = serde_json::from_str(json).unwrap();
        assert_eq!(resp.len(), 1);
        assert_eq!(resp[0].name, "rustfmt");
    }

    #[test]
    fn test_update_part_response_deserialize() {
        let json = r#"{"delta":"Hello"}"#;
        let resp: UpdatePartResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.delta, Some("Hello".to_string()));
    }

    #[test]
    fn test_permission_reply_response_deserialize() {
        let json = r#"{"sessionId":"sess-123","requestId":"req-456","reply":"always"}"#;
        let resp: PermissionReplyResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.session_id, Some("sess-123".to_string()));
        assert_eq!(resp.request_id, "req-456");
        assert_eq!(
            resp.reply,
            crate::types::permission::PermissionReply::Always
        );
    }
}
