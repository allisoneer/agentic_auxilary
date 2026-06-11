use crate::types::project::ModelRef;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "sessionID")]
    pub session_id: Option<String>,
    pub role: String,
    #[serde(default)]
    pub content: Vec<ContentPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<MessageTime>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTime {
    pub created: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTimeStart {
    pub start: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTimeRange {
    pub start: i64,
    pub end: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatePending {
    #[serde(deserialize_with = "deserialize_pending_status")]
    pub status: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub raw: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStateRunning {
    pub status: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub time: ToolTimeStart,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStateCompleted {
    pub status: String,
    #[serde(default)]
    pub input: serde_json::Value,
    pub output: String,
    pub title: String,
    pub time: ToolTimeRange,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStateError {
    pub status: String,
    #[serde(default)]
    pub input: serde_json::Value,
    pub error: String,
    pub time: ToolTimeRange,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolState {
    Completed(ToolStateCompleted),
    Error(ToolStateError),
    Running(ToolStateRunning),
    Pending(ToolStatePending),
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ContentPart {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    Tool {
        #[serde(rename = "callID")]
        call_id: String,
        tool: String,
        #[serde(default)]
        input: serde_json::Value,
        #[serde(default)]
        state: Option<ToolState>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    #[serde(rename = "step-finish")]
    StepFinish {
        reason: String,
        #[serde(default)]
        cost: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tokens: Option<TokenUsage>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

fn deserialize_pending_status<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let status = String::deserialize(deserializer)?;
    if status == "pending" {
        Ok(status)
    } else {
        Err(<D::Error as serde::de::Error>::custom(
            "expected pending tool status",
        ))
    }
}

impl Message {
    pub fn assistant_text(&self) -> Option<String> {
        let text = self
            .content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        (!text.trim().is_empty()).then_some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::ToolState;
    use super::ToolStatePending;
    use serde_json::json;

    #[test]
    fn tool_state_pending_deserializes_with_defaults() {
        let state: ToolState = serde_json::from_value(json!({ "status": "pending" })).unwrap();

        assert!(matches!(state, ToolState::Pending(ToolStatePending { .. })));
    }

    #[test]
    fn tool_state_pending_deserializes_with_payload() {
        let state: ToolState = serde_json::from_value(json!({
            "status": "pending",
            "input": {"tool": "read"},
            "raw": "read file.txt"
        }))
        .unwrap();

        assert!(matches!(state, ToolState::Pending(ToolStatePending { .. })));
    }

    #[test]
    fn tool_state_unknown_status_falls_through_to_unknown() {
        let state: ToolState = serde_json::from_value(json!({ "status": "queued" })).unwrap();

        assert!(
            matches!(state, ToolState::Unknown(value) if value == json!({ "status": "queued" }))
        );
    }
}
