//! Permission types for opencode_rs.

use serde::{Deserialize, Serialize};

/// A permission request pending user approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    /// Unique request identifier.
    pub request_id: String,
    /// Session ID associated with this permission.
    pub session_id: String,
    /// Permission type.
    pub r#type: String,
    /// Description of what is being requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tool name if this is a tool permission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// File path if this is a file permission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Command if this is a command permission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Timestamp when permission was requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
}

/// Reply to a permission request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionReply {
    /// Allow the requested action.
    Allow,
    /// Deny the requested action.
    Deny,
    /// Allow for this session only.
    AllowSession,
    /// Allow always for this type of action.
    AllowAlways,
}

/// Request to reply to a permission.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReplyRequest {
    /// The reply to send.
    pub reply: PermissionReply,
}

impl PermissionReply {
    /// Convert to the string format expected by the API.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::AllowSession => "allow_session",
            Self::AllowAlways => "allow_always",
        }
    }
}
