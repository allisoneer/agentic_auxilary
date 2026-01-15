//! Permission types for opencode_rs.
//!
//! Matches TypeScript PermissionNext schema from permission/next.ts.

use serde::{Deserialize, Serialize};

/// Permission action (allow, deny, ask).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

/// A permission rule in a ruleset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    /// Permission type (e.g., "file.read", "bash.execute").
    pub permission: String,
    /// Pattern to match (glob or regex).
    pub pattern: String,
    /// Action to take when matched.
    pub action: PermissionAction,
}

/// A ruleset is a list of permission rules.
pub type Ruleset = Vec<PermissionRule>;

/// Reference to a tool invocation for permission context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionToolRef {
    /// Message ID containing the tool call.
    pub message_id: String,
    /// Tool call ID.
    pub call_id: String,
}

/// A permission request from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    /// Unique request identifier.
    pub id: String,
    /// Session ID.
    pub session_id: String,
    /// Permission type being requested.
    pub permission: String,
    /// Patterns being requested.
    pub patterns: Vec<String>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Patterns that can be allowed "always".
    #[serde(default)]
    pub always: Vec<String>,
    /// Tool reference if this permission is for a tool invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<PermissionToolRef>,
}

/// Reply to a permission request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionReply {
    /// Allow once for this request.
    Once,
    /// Allow always for matching patterns.
    Always,
    /// Reject the permission request.
    Reject,
}

/// Request body for replying to a permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReplyRequest {
    /// The reply to send.
    pub reply: PermissionReply,
    /// Optional message to include with the reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_action_serialize() {
        assert_eq!(
            serde_json::to_string(&PermissionAction::Allow).unwrap(),
            r#""allow""#
        );
        assert_eq!(
            serde_json::to_string(&PermissionAction::Deny).unwrap(),
            r#""deny""#
        );
        assert_eq!(
            serde_json::to_string(&PermissionAction::Ask).unwrap(),
            r#""ask""#
        );
    }

    #[test]
    fn test_permission_rule_deserialize() {
        let json = r#"{"permission":"file.read","pattern":"**/*.rs","action":"allow"}"#;
        let rule: PermissionRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.permission, "file.read");
        assert_eq!(rule.pattern, "**/*.rs");
        assert_eq!(rule.action, PermissionAction::Allow);
    }

    #[test]
    fn test_ruleset_deserialize() {
        let json = r#"[
            {"permission":"file.read","pattern":"**/*.rs","action":"allow"},
            {"permission":"bash.execute","pattern":"*","action":"ask"}
        ]"#;
        let ruleset: Ruleset = serde_json::from_str(json).unwrap();
        assert_eq!(ruleset.len(), 2);
        assert_eq!(ruleset[0].action, PermissionAction::Allow);
        assert_eq!(ruleset[1].action, PermissionAction::Ask);
    }

    #[test]
    fn test_permission_request_deserialize() {
        let json = r#"{
            "id": "req-123",
            "sessionId": "sess-456",
            "permission": "file.write",
            "patterns": ["src/*.rs", "lib/*.rs"],
            "always": ["src/*.rs"],
            "tool": {"messageId": "msg-1", "callId": "call-1"}
        }"#;
        let req: PermissionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "req-123");
        assert_eq!(req.session_id, "sess-456");
        assert_eq!(req.permission, "file.write");
        assert_eq!(req.patterns.len(), 2);
        assert_eq!(req.always.len(), 1);
        assert!(req.tool.is_some());
        let tool = req.tool.unwrap();
        assert_eq!(tool.message_id, "msg-1");
        assert_eq!(tool.call_id, "call-1");
    }

    #[test]
    fn test_permission_request_minimal() {
        let json = r#"{
            "id": "req-123",
            "sessionId": "sess-456",
            "permission": "file.read",
            "patterns": ["**/*"]
        }"#;
        let req: PermissionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "req-123");
        assert!(req.always.is_empty());
        assert!(req.tool.is_none());
        assert!(req.metadata.is_none());
    }

    #[test]
    fn test_permission_reply_serialize() {
        assert_eq!(
            serde_json::to_string(&PermissionReply::Once).unwrap(),
            r#""once""#
        );
        assert_eq!(
            serde_json::to_string(&PermissionReply::Always).unwrap(),
            r#""always""#
        );
        assert_eq!(
            serde_json::to_string(&PermissionReply::Reject).unwrap(),
            r#""reject""#
        );
    }

    #[test]
    fn test_permission_reply_request_serialize() {
        let req = PermissionReplyRequest {
            reply: PermissionReply::Always,
            message: Some("User approved".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""reply":"always""#));
        assert!(json.contains(r#""message":"User approved""#));
    }

    #[test]
    fn test_permission_reply_request_minimal() {
        let json = r#"{"reply":"once"}"#;
        let req: PermissionReplyRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.reply, PermissionReply::Once);
        assert!(req.message.is_none());
    }
}
