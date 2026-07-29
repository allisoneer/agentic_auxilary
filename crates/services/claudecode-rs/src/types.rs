use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

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
            Self::Sonnet => write!(f, "sonnet"),
            Self::Opus => write!(f, "opus"),
            Self::Haiku => write!(f, "haiku"),
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
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::StreamingJson => write!(f, "stream-json"),
        }
    }
}

/// Permission mode for Claude CLI session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    DontAsk,
    Plan,
    BypassPermissions,
}

impl fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Default => "default",
            Self::AcceptEdits => "acceptEdits",
            Self::DontAsk => "dontAsk",
            Self::Plan => "plan",
            Self::BypassPermissions => "bypassPermissions",
        };
        write!(f, "{s}")
    }
}

/// Input format for Claude CLI session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Text,
    StreamJson,
}

impl fmt::Display for InputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::StreamJson => write!(f, "stream-json"),
        }
    }
}

#[derive(Debug, Clone)]
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
    /// Tool use whose input is not representable by the legacy object field shape.
    StructuredToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result carrying structured content or explicit error state.
    StructuredToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        is_error: bool,
    },
    Unknown(serde_json::Value),
}

impl<'de> Deserialize<'de> for Content {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value.get("type").and_then(serde_json::Value::as_str);
        match kind {
            Some("text") => Ok(Self::Text {
                text: required_string::<D::Error>(&value, "text")?,
            }),
            Some("tool_use") => {
                let id = required_string::<D::Error>(&value, "id")?;
                let name = required_string::<D::Error>(&value, "name")?;
                let input = value
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                match input {
                    serde_json::Value::Object(input) => Ok(Self::ToolUse {
                        id,
                        name,
                        input: input.into_iter().collect(),
                    }),
                    input => Ok(Self::StructuredToolUse { id, name, input }),
                }
            }
            Some("tool_result") => {
                let tool_use_id = required_string::<D::Error>(&value, "tool_use_id")?;
                let content = value
                    .get("content")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let is_error = value
                    .get("is_error")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                match content {
                    serde_json::Value::String(content) if !is_error => Ok(Self::ToolResult {
                        tool_use_id,
                        content,
                    }),
                    content => Ok(Self::StructuredToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    }),
                }
            }
            _ => Ok(Self::Unknown(value)),
        }
    }
}

fn required_string<E: serde::de::Error>(
    value: &serde_json::Value,
    key: &str,
) -> std::result::Result<String, E> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| serde::de::Error::custom(format!("missing string field `{key}`")))
}

impl Serialize for Content {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value = match self {
            Self::Text { text } => serde_json::json!({"type": "text", "text": text}),
            Self::ToolUse { id, name, input } => {
                serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": input})
            }
            Self::ToolResult {
                tool_use_id,
                content,
            } => serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
            }),
            Self::StructuredToolUse { id, name, input } => {
                serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": input})
            }
            Self::StructuredToolResult {
                tool_use_id,
                content,
                is_error,
            } => serde_json::json!({
                "type": "tool_result",
                "tool_use_id": tool_use_id,
                "content": content,
                "is_error": is_error,
            }),
            Self::Unknown(raw) => raw.clone(),
        };
        value.serialize(serializer)
    }
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
    #[serde(rename = "user")]
    User(UserEvent),
    #[serde(rename = "result")]
    Result(ResultEvent),
    #[serde(rename = "error")]
    Error(ErrorEvent),
    #[serde(other)]
    Unknown,
}

/// One parsed event together with its exact original JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    pub raw: serde_json::Value,
    pub event: Event,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvocationMetadata {
    pub sdk_version: String,
    pub claude_version: Option<String>,
    pub argv: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub setting_sources: Option<Vec<String>>,
    pub environment_keys: Vec<String>,
    pub mcp_config: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct SessionOutcome {
    pub result: Result,
    pub transcript: Vec<RawEvent>,
    pub exit_code: Option<i32>,
    pub raw_stdout: String,
    pub stderr: String,
    pub invocation: InvocationMetadata,
}

#[derive(Debug, Clone)]
pub struct SessionFailure {
    pub message: String,
    pub transcript: Vec<RawEvent>,
    pub exit_code: Option<i32>,
    pub raw_stdout: String,
    pub stderr: String,
    pub invocation: InvocationMetadata,
}

impl RawEvent {
    pub fn from_value(raw: serde_json::Value) -> serde_json::Result<Self> {
        let event = serde_json::from_value(raw.clone())?;
        Ok(Self { raw, event })
    }
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

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_server_errors: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantEvent {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserEvent {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tool_use_id: Option<String>,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEvent {
    pub session_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,

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
        Self::Text { text: text.into() }
    }

    pub fn get_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text.as_str()),
            _ => None,
        }
    }

    pub fn tool_result(&self) -> Option<(&str, serde_json::Value, bool)> {
        match self {
            Self::ToolResult {
                tool_use_id,
                content,
            } => Some((
                tool_use_id,
                serde_json::Value::String(content.clone()),
                false,
            )),
            Self::StructuredToolResult {
                tool_use_id,
                content,
                is_error,
            } => Some((tool_use_id, content.clone(), *is_error)),
            _ => None,
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
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

        // Test user tool-result event
        let json = r#"{"type":"user","session_id":"123","parent_tool_use_id":"parent-1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":{"nonce":"ok"},"is_error":false}]}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        match event {
            Event::User(user) => {
                assert_eq!(user.session_id, "123");
                assert_eq!(user.parent_tool_use_id.as_deref(), Some("parent-1"));
                let (tool_use_id, content, is_error) =
                    user.message.content[0].tool_result().unwrap();
                assert_eq!(tool_use_id, "call-1");
                assert_eq!(content, serde_json::json!({"nonce": "ok"}));
                assert!(!is_error);
            }
            _ => panic!("Expected User event"),
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
    fn raw_event_preserves_unknown_top_level_payload() {
        let raw = serde_json::json!({"type": "future", "nonce": 42});
        let envelope = RawEvent::from_value(raw.clone()).unwrap();
        assert!(matches!(envelope.event, Event::Unknown));
        assert_eq!(envelope.raw, raw);
    }

    #[test]
    fn init_preserves_nonempty_mcp_server_errors() {
        let raw = serde_json::json!({
            "type": "system",
            "subtype": "init",
            "session_id": "s",
            "mcp_server_errors": [{"server": "nested", "error": "failed"}]
        });
        let envelope = RawEvent::from_value(raw.clone()).unwrap();
        let Event::System(system) = envelope.event else {
            panic!("expected system event");
        };
        assert_eq!(
            system.mcp_server_errors,
            vec![raw["mcp_server_errors"][0].clone()]
        );
    }

    #[test]
    fn tool_result_accepts_string_and_structured_content() {
        for (json, expected) in [
            (
                r#"{"type":"tool_result","tool_use_id":"call-1","content":"ok"}"#,
                serde_json::json!("ok"),
            ),
            (
                r#"{"type":"tool_result","tool_use_id":"call-1","content":[{"type":"text","text":"ok"}],"is_error":true}"#,
                serde_json::json!([{"type":"text","text":"ok"}]),
            ),
        ] {
            let content: Content = serde_json::from_str(json).unwrap();
            let (_, actual, _) = content.tool_result().unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn unknown_content_block_preserves_raw_payload() {
        let raw = serde_json::json!({"type": "future_block", "nonce": 7});
        let content: Content = serde_json::from_value(raw.clone()).unwrap();
        assert!(matches!(content, Content::Unknown(value) if value == raw));
    }

    #[test]
    fn test_content_helpers() {
        let content = Content::text("Hello, world!");
        assert!(content.is_text());
        assert_eq!(content.get_text(), Some("Hello, world!"));
        assert!(matches!(content, Content::Text { .. }));
    }

    #[test]
    fn test_permission_mode_display() {
        assert_eq!(PermissionMode::Default.to_string(), "default");
        assert_eq!(PermissionMode::AcceptEdits.to_string(), "acceptEdits");
        assert_eq!(PermissionMode::DontAsk.to_string(), "dontAsk");
        assert_eq!(PermissionMode::Plan.to_string(), "plan");
        assert_eq!(
            PermissionMode::BypassPermissions.to_string(),
            "bypassPermissions"
        );
    }

    #[test]
    fn test_input_format_display() {
        assert_eq!(InputFormat::Text.to_string(), "text");
        assert_eq!(InputFormat::StreamJson.to_string(), "stream-json");
    }
}
