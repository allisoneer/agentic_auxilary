use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Model {
    #[serde(rename = "sonnet")]
    Sonnet,
    #[serde(rename = "opus")]
    Opus,
    #[serde(rename = "haiku")]
    Haiku,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Model::Sonnet => write!(f, "sonnet"),
            Model::Opus => write!(f, "opus"),
            Model::Haiku => write!(f, "haiku"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    Text,
    Json,
    #[default]
    StreamingJson,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Text => write!(f, "text"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::StreamingJson => write!(f, "stream-json"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: HashMap<String, serde_json::Value>,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,

    pub role: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    pub content: Vec<Content>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolUse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_requests: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: i32,
    pub output_tokens: i32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<ServerToolUse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPStatus {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Result {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(default)]
    pub is_error: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost_usd: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_api_ms: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_turns: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

// Type-safe event system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Event {
    #[serde(rename = "system")]
    System(SystemEvent),
    #[serde(rename = "assistant")]
    Assistant(AssistantEvent),
    #[serde(rename = "result")]
    Result(ResultEvent),
    #[serde(rename = "error")]
    Error(ErrorEvent),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub session_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,

    // Init subtype fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "permissionMode")]
    pub permission_mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "apiKeySource")]
    pub api_key_source: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<MCPStatus>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantEvent {
    pub session_id: String,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEvent {
    pub session_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,

    #[serde(default)]
    pub is_error: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost_usd: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_api_ms: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_turns: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub session_id: String,
    pub error: String,
}

// Helper methods for Content
impl Content {
    pub fn text(text: impl Into<String>) -> Self {
        Content::Text { text: text.into() }
    }

    pub fn get_text(&self) -> Option<&str> {
        match self {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Content::Text { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_serialization() {
        let model = Model::Sonnet;
        assert_eq!(model.to_string(), "sonnet");

        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, "\"sonnet\"");

        let deserialized: Model = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, model);

        // Test Opus
        let model = Model::Opus;
        assert_eq!(model.to_string(), "opus");

        // Test Haiku
        let model = Model::Haiku;
        assert_eq!(model.to_string(), "haiku");

        let json = serde_json::to_string(&model).unwrap();
        assert_eq!(json, "\"haiku\"");

        let deserialized: Model = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, model);
    }

    #[test]
    fn test_output_format_display() {
        assert_eq!(OutputFormat::Text.to_string(), "text");
        assert_eq!(OutputFormat::Json.to_string(), "json");
        assert_eq!(OutputFormat::StreamingJson.to_string(), "stream-json");
    }

    #[test]
    fn test_default_output_format() {
        let default = OutputFormat::default();
        assert_eq!(default, OutputFormat::StreamingJson);
    }

    #[test]
    fn test_result_default() {
        let result = Result::default();
        assert!(!result.is_error);
        assert!(result.content.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_event_deserialization() {
        // Test system event
        let json = r#"{"type":"system","session_id":"123","subtype":"init","cwd":"/home/user"}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::System(sys) => {
                assert_eq!(sys.session_id, "123");
                assert_eq!(sys.subtype, Some("init".to_string()));
                assert_eq!(sys.cwd, Some("/home/user".to_string()));
            }
            _ => panic!("Expected System event"),
        }

        // Test assistant event
        let json = r#"{"type":"assistant","session_id":"123","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::Assistant(asst) => {
                assert_eq!(asst.session_id, "123");
                assert_eq!(asst.message.role, "assistant");
                assert_eq!(asst.message.content.len(), 1);
                assert_eq!(asst.message.content[0].get_text(), Some("Hello"));
            }
            _ => panic!("Expected Assistant event"),
        }

        // Test result event
        let json = r#"{"type":"result","session_id":"123","total_cost_usd":0.05,"num_turns":2}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::Result(res) => {
                assert_eq!(res.session_id, "123");
                assert_eq!(res.total_cost_usd, Some(0.05));
                assert_eq!(res.num_turns, Some(2));
                assert!(!res.is_error);
            }
            _ => panic!("Expected Result event"),
        }

        // Test error event
        let json = r#"{"type":"error","session_id":"123","error":"Something went wrong"}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::Error(err) => {
                assert_eq!(err.session_id, "123");
                assert_eq!(err.error, "Something went wrong");
            }
            _ => panic!("Expected Error event"),
        }

        // Test unknown event type
        let json = r#"{"type":"unknown_type","session_id":"123"}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::Unknown));
    }

    #[test]
    fn test_content_helpers() {
        let content = Content::text("Hello, world!");
        assert!(content.is_text());
        assert_eq!(content.get_text(), Some("Hello, world!"));
        assert!(matches!(content, Content::Text { .. }));
    }
}
