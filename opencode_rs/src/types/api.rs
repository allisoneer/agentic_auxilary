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

/// Response from LSP status endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspStatus {
    /// LSP servers (structure varies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servers: Option<serde_json::Value>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response from formatter status endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatterStatus {
    /// Whether formatter is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
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
    fn test_lsp_status_deserialize() {
        let json = r#"{"servers":{"rust-analyzer":{"status":"running"}}}"#;
        let resp: LspStatus = serde_json::from_str(json).unwrap();
        assert!(resp.servers.is_some());
    }

    #[test]
    fn test_formatter_status_deserialize() {
        let json = r#"{"enabled":true}"#;
        let resp: FormatterStatus = serde_json::from_str(json).unwrap();
        assert_eq!(resp.enabled, Some(true));
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
