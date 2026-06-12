//! V2 permission endpoint types.

pub type PermissionReply = crate::types::permission::PermissionReply;
pub type PermissionReplyRequest = crate::types::permission::PermissionReplyRequest;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSource {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "messageID")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "callID")]
    pub call_id: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub action: String,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PermissionSource>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
