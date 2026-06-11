use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionToolRef {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "messageID")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "callID")]
    pub call_id: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub action: String,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<PermissionToolRef>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionReply {
    Once,
    Always,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionReplyRequest {
    pub reply: PermissionReply,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
