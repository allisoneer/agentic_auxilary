//! Permission types for `opencode_rs`.
//!
//! Matches TypeScript `PermissionNext` schema from permission/next.ts.

use serde::Deserialize;
use serde::Serialize;
use serde::de::Deserializer;

/// Custom deserializer for `Option<PermissionToolRef>` that handles edge cases:
/// - `null` → `None`
/// - `{}` (empty object) → `None`
/// - Partial object (missing `messageId` or `callId`) → `None`
/// - Valid object with both fields → `Some(PermissionToolRef)`
///
/// This is necessary because `OpenCode` may send `"tool": {}` or partial objects
/// in certain scenarios (doom loop detection, subtask execution) where no complete
/// tool context exists. We treat any malformed tool as "no tool context" rather
/// than failing the entire permission list deserialization.
fn deserialize_tool_ref_opt<'de, D>(deserializer: D) -> Result<Option<PermissionToolRef>, D::Error>
where
    D: Deserializer<'de>,
{
    // Deserialize into an optional JSON value so we can inspect the structure
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;

    let Some(value) = value else {
        return Ok(None);
    };

    match value {
        serde_json::Value::Object(map) => {
            // Extract messageID and callID (OpenCode uses uppercase ID suffix)
            let message_id = map.get("messageID").and_then(|v| v.as_str());

            let call_id = map.get("callID").and_then(|v| v.as_str());

            match (message_id, call_id) {
                (Some(mid), Some(cid)) => Ok(Some(PermissionToolRef {
                    message_id: mid.to_owned(),
                    call_id: cid.to_owned(),
                })),
                // Empty, partial, or malformed → treat as absent tool context
                _ => Ok(None),
            }
        }
        // Non-object types (unexpected) → treat as absent
        _ => Ok(None),
    }
}

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
pub struct PermissionToolRef {
    /// Message ID containing the tool call.
    #[serde(rename = "messageID")]
    pub message_id: String,
    /// Tool call ID.
    #[serde(rename = "callID")]
    pub call_id: String,
}

/// A permission request from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    /// Unique request identifier.
    pub id: String,
    /// Session ID.
    #[serde(rename = "sessionID")]
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
    ///
    /// Uses custom deserializer to handle edge cases where `OpenCode` sends
    /// `"tool": {}` or `"tool": null` instead of omitting the field.
    #[serde(
        default,
        deserialize_with = "deserialize_tool_ref_opt",
        skip_serializing_if = "Option::is_none"
    )]
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
            "sessionID": "sess-456",
            "permission": "file.write",
            "patterns": ["src/*.rs", "lib/*.rs"],
            "always": ["src/*.rs"],
            "tool": {"messageID": "msg-1", "callID": "call-1"}
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
            "sessionID": "sess-456",
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

    #[test]
    fn test_permission_request_tool_absent_deserializes_to_none() {
        // Tool field completely missing from JSON
        let json = r#"{
            "id": "perm_test123",
            "sessionID": "sess_abc",
            "permission": "file.write",
            "patterns": ["/tmp/**"]
        }"#;

        let req: PermissionRequest =
            serde_json::from_str(json).expect("should deserialize without tool field");
        assert!(
            req.tool.is_none(),
            "absent tool field should deserialize to None"
        );
    }

    #[test]
    fn test_permission_request_tool_null_deserializes_to_none() {
        // Tool field explicitly set to null
        let json = r#"{
            "id": "perm_test123",
            "sessionID": "sess_abc",
            "permission": "file.write",
            "patterns": ["/tmp/**"],
            "tool": null
        }"#;

        let req: PermissionRequest =
            serde_json::from_str(json).expect("should deserialize with null tool");
        assert!(req.tool.is_none(), "null tool should deserialize to None");
    }

    #[test]
    fn test_permission_request_tool_empty_object_deserializes_to_none() {
        // Tool field set to empty object {} - the problematic case from OpenCode
        let json = r#"{
            "id": "perm_test123",
            "sessionID": "sess_abc",
            "permission": "file.write",
            "patterns": ["/tmp/**"],
            "tool": {}
        }"#;

        let req: PermissionRequest =
            serde_json::from_str(json).expect("should deserialize with empty tool object");
        assert!(
            req.tool.is_none(),
            "empty object tool should deserialize to None"
        );
    }

    #[test]
    fn test_permission_request_tool_valid_object_deserializes_to_some() {
        // Tool field with valid messageId and callId
        let json = r#"{
            "id": "perm_test123",
            "sessionID": "sess_abc",
            "permission": "file.write",
            "patterns": ["/tmp/**"],
            "tool": {
                "messageID": "msg_xyz",
                "callID": "call_789"
            }
        }"#;

        let req: PermissionRequest =
            serde_json::from_str(json).expect("should deserialize with valid tool");

        let tool = req.tool.expect("tool should be Some");
        assert_eq!(tool.message_id, "msg_xyz");
        assert_eq!(tool.call_id, "call_789");
    }

    #[test]
    fn test_permission_request_tool_partial_object_deserializes_to_none() {
        // Tool field with only one of the required fields - should become None
        // This ensures robustness: partial/malformed tool context doesn't crash deserialization
        let json = r#"{
            "id": "perm_test123",
            "sessionID": "sess_abc",
            "permission": "file.write",
            "patterns": ["/tmp/**"],
            "tool": {
                "messageID": "msg_xyz"
            }
        }"#;

        let req: PermissionRequest =
            serde_json::from_str(json).expect("should deserialize with partial tool");
        assert!(
            req.tool.is_none(),
            "partial tool object (missing callId) should deserialize to None"
        );
    }

    #[test]
    fn test_permission_request_tool_partial_object_missing_message_id() {
        // Tool field with only callId - should become None
        let json = r#"{
            "id": "perm_test123",
            "sessionID": "sess_abc",
            "permission": "file.write",
            "patterns": ["/tmp/**"],
            "tool": {
                "callID": "call_789"
            }
        }"#;

        let req: PermissionRequest =
            serde_json::from_str(json).expect("should deserialize with partial tool");
        assert!(
            req.tool.is_none(),
            "partial tool object (missing messageId) should deserialize to None"
        );
    }
}
