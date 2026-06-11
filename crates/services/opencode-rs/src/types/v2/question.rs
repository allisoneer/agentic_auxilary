use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionRequest {
    pub id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    pub questions: Vec<QuestionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<QuestionToolContext>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInfo {
    pub question: String,
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default = "default_true")]
    pub custom: bool,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionToolContext {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "messageID")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "callID")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionReply {
    pub answers: Vec<Vec<String>>,
}

fn default_true() -> bool {
    true
}
