//! Message and content part types for `opencode_rs`.
//!
// TODO(3): Add unit tests for Message, Part variants, and PromptPart serialization/deserialization
// TODO(3): Consider using enum for `role` field (User/Assistant/System) with #[serde(other)] for forward-compat

use serde::{Deserialize, Serialize};

/// Message info (metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageInfo {
    /// Unique message identifier.
    pub id: String,
    /// Session ID (may be omitted when context implies it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Message role (user, assistant, system).
    pub role: String,
    /// Message timestamps.
    pub time: MessageTime,
    /// Agent name if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Message variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,

    // Upstream parity fields
    /// Message format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Model reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<crate::types::project::ModelRef>,
    /// System prompt override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Tools available for this message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Parent message ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Model ID (denormalized).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Provider ID (denormalized).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Path context for this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<MessagePath>,
    /// Cost of this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// Token usage for this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,
    /// Structured output/response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured: Option<serde_json::Value>,
    /// Finish reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    /// Additional fields from server (forward compatibility).
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Path context for a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePath {
    /// Current working directory.
    pub cwd: String,
    /// Root directory.
    pub root: String,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Typed view of message info based on role.
pub enum MessageInfoKind<'a> {
    /// User message.
    User(&'a MessageInfo),
    /// Assistant message.
    Assistant(&'a MessageInfo),
    /// System message.
    System(&'a MessageInfo),
    /// Unknown/other role.
    Other(&'a MessageInfo),
}

impl MessageInfo {
    /// Get a typed view of this message based on its role.
    pub fn kind(&self) -> MessageInfoKind<'_> {
        match self.role.as_str() {
            "user" => MessageInfoKind::User(self),
            "assistant" => MessageInfoKind::Assistant(self),
            "system" => MessageInfoKind::System(self),
            _ => MessageInfoKind::Other(self),
        }
    }
}

/// Message timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTime {
    /// Creation timestamp.
    pub created: i64,
    /// Completion timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<i64>,
}

/// A message with its parts (API response format).
///
/// This is the format returned by the message list endpoint.
/// It contains a nested `info` object and a `parts` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Message info/metadata.
    pub info: MessageInfo,
    /// Content parts.
    pub parts: Vec<Part>,
}

impl Message {
    /// Get the message ID.
    pub fn id(&self) -> &str {
        &self.info.id
    }

    /// Get the session ID if present.
    pub fn session_id(&self) -> Option<&str> {
        self.info.session_id.as_deref()
    }

    /// Get the message role.
    pub fn role(&self) -> &str {
        &self.info.role
    }
}

/// Alias for backward compatibility.
pub type MessageWithParts = Message;

/// A content part within a message (12 variants).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Part {
    /// Text content.
    Text {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Text content.
        text: String,
        /// Whether this is synthetic (generated).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        synthetic: Option<bool>,
        /// Whether this part is ignored.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ignored: Option<bool>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    /// File attachment.
    File {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// MIME type.
        mime: String,
        /// File URL.
        url: String,
        /// Original filename.
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        /// File source info.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<FilePartSource>,
    },
    /// Tool invocation.
    Tool {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Tool call ID.
        #[serde(rename = "callID")]
        call_id: String,
        /// Tool name.
        tool: String,
        /// Tool input arguments.
        #[serde(default)]
        input: serde_json::Value,
        /// Tool execution state.
        #[serde(default)]
        state: Option<ToolState>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    /// Reasoning/thinking content.
    Reasoning {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Reasoning text.
        text: String,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    /// Step start marker.
    #[serde(rename = "step-start")]
    StepStart {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Snapshot ID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snapshot: Option<String>,
    },
    /// Step finish marker.
    #[serde(rename = "step-finish")]
    StepFinish {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Finish reason.
        reason: String,
        /// Snapshot ID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snapshot: Option<String>,
        /// Cost incurred.
        #[serde(default)]
        cost: f64,
        /// Token usage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tokens: Option<TokenUsage>,
    },
    /// Snapshot marker.
    Snapshot {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Snapshot ID.
        snapshot: String,
    },
    /// Patch information.
    Patch {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Patch hash.
        hash: String,
        /// Affected files.
        #[serde(default)]
        files: Vec<String>,
    },
    /// Agent delegation.
    Agent {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Agent name.
        name: String,
        /// Agent source info.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<AgentSource>,
    },
    /// Retry marker.
    Retry {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Attempt number.
        attempt: u32,
        /// Error that caused retry.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<crate::types::error::APIError>,
    },
    /// Compaction marker.
    Compaction {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Whether this was automatic.
        #[serde(default)]
        auto: bool,
    },
    /// Subtask delegation.
    Subtask {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Subtask prompt.
        prompt: String,
        /// Subtask description.
        description: String,
        /// Agent to handle subtask.
        agent: String,
        /// Optional command.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
    /// Unknown part type (forward compatibility).
    #[serde(other)]
    Unknown,
}

/// Agent source information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSource {
    /// Source value.
    pub value: String,
    /// Start offset.
    pub start: i64,
    /// End offset.
    pub end: i64,
}

// ==================== FilePartSource ====================

/// Text range within a file part source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePartSourceText {
    /// The text content.
    pub value: String,
    /// Start offset in file.
    pub start: i64,
    /// End offset in file.
    pub end: i64,
}

/// Source information for a file part (internally tagged by "type").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type")]
pub enum FilePartSource {
    /// File source.
    #[serde(rename = "file")]
    File {
        /// Text range.
        text: FilePartSourceText,
        /// File path.
        path: String,
        /// Additional fields.
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Symbol source (from LSP).
    #[serde(rename = "symbol")]
    Symbol {
        /// Text range.
        text: FilePartSourceText,
        /// File path.
        path: String,
        /// LSP range (kept as Value for now).
        range: serde_json::Value,
        /// Symbol name.
        name: String,
        /// Symbol kind (LSP `SymbolKind`).
        kind: i64,
        /// Additional fields.
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// MCP resource source.
    #[serde(rename = "resource")]
    Resource {
        /// Text range.
        text: FilePartSourceText,
        /// MCP client name.
        #[serde(rename = "clientName")]
        client_name: String,
        /// Resource URI.
        uri: String,
        /// Additional fields.
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Unknown source type (forward compatibility).
    #[serde(other)]
    Unknown,
}

// ==================== ToolState ====================

/// Tool execution time (start only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTimeStart {
    /// Start timestamp (ms).
    pub start: i64,
}

/// Tool execution time range.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTimeRange {
    /// Start timestamp (ms).
    pub start: i64,
    /// End timestamp (ms).
    pub end: i64,
    /// Compacted timestamp if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacted: Option<i64>,
}

/// Tool state when pending execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatePending {
    /// Status field (always "pending").
    pub status: String,
    /// Tool input arguments.
    pub input: serde_json::Value,
    /// Raw input string.
    pub raw: String,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Tool state when running.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStateRunning {
    /// Status field (always "running").
    pub status: String,
    /// Tool input arguments.
    pub input: serde_json::Value,
    /// Display title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Execution time.
    pub time: ToolTimeStart,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Tool state when completed successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStateCompleted {
    /// Status field (always "completed").
    pub status: String,
    /// Tool input arguments.
    pub input: serde_json::Value,
    /// Tool output.
    pub output: String,
    /// Display title.
    pub title: String,
    /// Additional metadata.
    pub metadata: serde_json::Value,
    /// Execution time range.
    pub time: ToolTimeRange,
    /// File attachments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<serde_json::Value>>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Tool state when errored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStateError {
    /// Status field (always "error").
    pub status: String,
    /// Tool input arguments.
    pub input: serde_json::Value,
    /// Error message.
    pub error: String,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    /// Execution time range.
    pub time: ToolTimeRange,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// State of a tool execution (untagged enum with Unknown fallback).
///
/// Variant order matters for untagged enums - most specific variants with more
/// required fields must come first to avoid less specific variants matching early.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(untagged)]
pub enum ToolState {
    /// Tool completed successfully.
    Completed(ToolStateCompleted),
    /// Tool encountered an error.
    Error(ToolStateError),
    /// Tool is currently running.
    Running(ToolStateRunning),
    /// Tool is pending execution.
    Pending(ToolStatePending),
    /// Unknown state (forward compatibility).
    Unknown(serde_json::Value),
}

impl ToolState {
    /// Get the status string for this tool state.
    pub fn status(&self) -> &str {
        match self {
            Self::Pending(s) => &s.status,
            Self::Running(s) => &s.status,
            Self::Completed(s) => &s.status,
            Self::Error(s) => &s.status,
            Self::Unknown(_) => "unknown",
        }
    }

    /// Get the output if the tool completed successfully.
    pub fn output(&self) -> Option<&str> {
        match self {
            Self::Completed(s) => Some(&s.output),
            _ => None,
        }
    }

    /// Get the error message if the tool errored.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Error(s) => Some(&s.error),
            _ => None,
        }
    }

    /// Check if the tool is pending.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending(_))
    }

    /// Check if the tool is running.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }

    /// Check if the tool completed successfully.
    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }

    /// Check if the tool errored.
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    /// Total tokens (sum of input + output + reasoning).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Input tokens.
    pub input: u64,
    /// Output tokens.
    pub output: u64,
    /// Reasoning tokens.
    #[serde(default)]
    pub reasoning: u64,
    /// Cache usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheUsage>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Cache usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheUsage {
    /// Cache read tokens.
    pub read: u64,
    /// Cache write tokens.
    pub write: u64,
}

/// Request to send a prompt to a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    /// Content parts to send.
    pub parts: Vec<PromptPart>,
    /// Message ID to reply to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Model to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<crate::types::project::ModelRef>,
    /// Agent to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Whether to skip reply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_reply: Option<bool>,
    /// System prompt override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Message variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

/// A content part in a prompt request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PromptPart {
    /// Text content.
    Text {
        /// Text content.
        text: String,
        /// Whether this is synthetic.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        synthetic: Option<bool>,
        /// Whether this part is ignored.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ignored: Option<bool>,
        /// Additional metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<serde_json::Value>,
    },
    /// File attachment.
    File {
        /// MIME type.
        mime: String,
        /// File URL.
        url: String,
        /// Original filename.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
    /// Agent delegation.
    Agent {
        /// Agent name.
        name: String,
    },
    /// Subtask delegation.
    Subtask {
        /// Subtask prompt.
        prompt: String,
        /// Subtask description.
        description: String,
        /// Agent to handle subtask.
        agent: String,
        /// Optional command.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
    },
}

/// Request to execute a command in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    /// Command to execute.
    pub command: String,
    /// Command arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Request to execute a shell command in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellRequest {
    /// Shell command to execute.
    pub command: String,
    /// Model to use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<crate::types::project::ModelRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_part_text_deserialize() {
        let json = r#"{"type":"text","id":"p1","text":"hello"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert!(matches!(part, Part::Text { text, .. } if text == "hello"));
    }

    #[test]
    fn test_part_tool_deserialize() {
        let json = r#"{"type":"tool","callID":"c1","tool":"read_file","input":{}}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert!(matches!(part, Part::Tool { tool, .. } if tool == "read_file"));
    }

    #[test]
    fn test_part_step_start_deserialize() {
        let json = r#"{"type":"step-start"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert!(matches!(part, Part::StepStart { .. }));
    }

    #[test]
    fn test_part_step_finish_deserialize() {
        let json = r#"{"type":"step-finish","reason":"done","cost":0.01}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert!(matches!(part, Part::StepFinish { reason, .. } if reason == "done"));
    }

    #[test]
    fn test_part_unknown_deserialize() {
        let json = r#"{"type":"future-part-type","data":"whatever"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        assert!(matches!(part, Part::Unknown));
    }

    // ==================== ToolState Tests ====================

    #[test]
    fn test_tool_state_pending() {
        let json = r#"{
            "status": "pending",
            "input": {"file": "test.rs"},
            "raw": "read test.rs"
        }"#;
        let state: ToolState = serde_json::from_str(json).unwrap();
        assert!(state.is_pending());
        assert_eq!(state.status(), "pending");
        assert!(state.output().is_none());
    }

    #[test]
    fn test_tool_state_running() {
        let json = r#"{
            "status": "running",
            "input": {"file": "test.rs"},
            "title": "Reading file",
            "time": {"start": 1234567890}
        }"#;
        let state: ToolState = serde_json::from_str(json).unwrap();
        assert!(state.is_running());
        assert_eq!(state.status(), "running");
    }

    #[test]
    fn test_tool_state_completed() {
        let json = r#"{
            "status": "completed",
            "input": {"file": "test.rs"},
            "output": "file contents here",
            "title": "Read test.rs",
            "metadata": {},
            "time": {"start": 1234567890, "end": 1234567900}
        }"#;
        let state: ToolState = serde_json::from_str(json).unwrap();
        assert!(state.is_completed());
        assert_eq!(state.status(), "completed");
        assert_eq!(state.output(), Some("file contents here"));
    }

    #[test]
    fn test_tool_state_error() {
        let json = r#"{
            "status": "error",
            "input": {"file": "missing.rs"},
            "error": "File not found",
            "time": {"start": 1234567890, "end": 1234567900}
        }"#;
        let state: ToolState = serde_json::from_str(json).unwrap();
        assert!(state.is_error());
        assert_eq!(state.status(), "error");
        assert_eq!(state.error(), Some("File not found"));
    }

    #[test]
    fn test_tool_state_unknown() {
        let json = r#"{
            "status": "future-status",
            "someField": "someValue"
        }"#;
        let state: ToolState = serde_json::from_str(json).unwrap();
        assert!(matches!(state, ToolState::Unknown(_)));
        assert_eq!(state.status(), "unknown");
    }

    // ==================== FilePartSource Tests ====================

    #[test]
    fn test_file_part_source_file() {
        let json = r#"{
            "type": "file",
            "text": {"value": "content", "start": 0, "end": 100},
            "path": "/src/main.rs"
        }"#;
        let source: FilePartSource = serde_json::from_str(json).unwrap();
        assert!(matches!(source, FilePartSource::File { path, .. } if path == "/src/main.rs"));
    }

    #[test]
    fn test_file_part_source_symbol() {
        let json = r#"{
            "type": "symbol",
            "text": {"value": "fn main()", "start": 10, "end": 20},
            "path": "/src/main.rs",
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 5, "character": 1}},
            "name": "main",
            "kind": 12
        }"#;
        let source: FilePartSource = serde_json::from_str(json).unwrap();
        assert!(
            matches!(source, FilePartSource::Symbol { name, kind, .. } if name == "main" && kind == 12)
        );
    }

    #[test]
    fn test_file_part_source_resource() {
        let json = r#"{
            "type": "resource",
            "text": {"value": "resource content", "start": 0, "end": 50},
            "clientName": "my-mcp-server",
            "uri": "resource://data/file.txt"
        }"#;
        let source: FilePartSource = serde_json::from_str(json).unwrap();
        assert!(
            matches!(source, FilePartSource::Resource { client_name, uri, .. } 
            if client_name == "my-mcp-server" && uri == "resource://data/file.txt")
        );
    }

    #[test]
    fn test_file_part_source_unknown() {
        let json = r#"{
            "type": "future-source",
            "data": "whatever"
        }"#;
        let source: FilePartSource = serde_json::from_str(json).unwrap();
        assert!(matches!(source, FilePartSource::Unknown));
    }

    #[test]
    fn test_file_part_source_with_extra_fields() {
        let json = r#"{
            "type": "file",
            "text": {"value": "content", "start": 0, "end": 100},
            "path": "/src/main.rs",
            "newField": "preserved"
        }"#;
        let source: FilePartSource = serde_json::from_str(json).unwrap();
        if let FilePartSource::File { extra, .. } = source {
            assert_eq!(extra.get("newField").unwrap(), "preserved");
        } else {
            panic!("Expected FilePartSource::File");
        }
    }

    // ==================== MessageInfo Tests ====================

    #[test]
    fn test_message_info_minimal() {
        let json = r#"{
            "id": "msg-123",
            "role": "user",
            "time": {"created": 1234567890}
        }"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, "msg-123");
        assert_eq!(info.role, "user");
        assert!(info.tokens.is_none());
        assert!(info.cost.is_none());
    }

    #[test]
    fn test_message_info_with_new_fields() {
        let json = r#"{
            "id": "msg-123",
            "sessionId": "sess-456",
            "role": "assistant",
            "time": {"created": 1234567890, "completed": 1234567900},
            "format": "markdown",
            "model": {"providerId": "anthropic", "modelId": "claude-3"},
            "system": "You are a helpful assistant",
            "tools": ["read_file", "write_file"],
            "parentId": "msg-100",
            "modelId": "claude-3",
            "providerId": "anthropic",
            "cost": 0.0125,
            "tokens": {"total": 1500, "input": 1000, "output": 500, "reasoning": 0},
            "finish": "stop"
        }"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.id, "msg-123");
        assert_eq!(info.session_id, Some("sess-456".to_string()));
        assert_eq!(info.role, "assistant");
        assert_eq!(info.format, Some("markdown".to_string()));
        assert!(info.model.is_some());
        assert_eq!(
            info.model.as_ref().unwrap().provider_id,
            Some("anthropic".to_string())
        );
        assert_eq!(info.system, Some("You are a helpful assistant".to_string()));
        assert_eq!(info.tools.len(), 2);
        assert_eq!(info.parent_id, Some("msg-100".to_string()));
        assert_eq!(info.model_id, Some("claude-3".to_string()));
        assert_eq!(info.provider_id, Some("anthropic".to_string()));
        assert_eq!(info.cost, Some(0.0125));
        assert!(info.tokens.is_some());
        let tokens = info.tokens.unwrap();
        assert_eq!(tokens.total, Some(1500));
        assert_eq!(tokens.input, 1000);
        assert_eq!(tokens.output, 500);
        assert_eq!(info.finish, Some("stop".to_string()));
    }

    #[test]
    fn test_message_info_with_path() {
        let json = r#"{
            "id": "msg-123",
            "role": "user",
            "time": {"created": 1234567890},
            "path": {"cwd": "/home/user/project", "root": "/home/user"}
        }"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert!(info.path.is_some());
        let path = info.path.unwrap();
        assert_eq!(path.cwd, "/home/user/project");
        assert_eq!(path.root, "/home/user");
    }

    #[test]
    fn test_message_info_extra_fields_preserved() {
        let json = r#"{
            "id": "msg-123",
            "role": "user",
            "time": {"created": 1234567890},
            "futureField": "preserved"
        }"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.extra.get("futureField").unwrap(), "preserved");
    }

    #[test]
    fn test_message_info_kind_user() {
        let json = r#"{"id": "m1", "role": "user", "time": {"created": 1}}"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert!(matches!(info.kind(), MessageInfoKind::User(_)));
    }

    #[test]
    fn test_message_info_kind_assistant() {
        let json = r#"{"id": "m1", "role": "assistant", "time": {"created": 1}}"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert!(matches!(info.kind(), MessageInfoKind::Assistant(_)));
    }

    #[test]
    fn test_message_info_kind_system() {
        let json = r#"{"id": "m1", "role": "system", "time": {"created": 1}}"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert!(matches!(info.kind(), MessageInfoKind::System(_)));
    }

    #[test]
    fn test_message_info_kind_other() {
        let json = r#"{"id": "m1", "role": "tool", "time": {"created": 1}}"#;
        let info: MessageInfo = serde_json::from_str(json).unwrap();
        assert!(matches!(info.kind(), MessageInfoKind::Other(_)));
    }

    #[test]
    fn test_token_usage_with_total() {
        let json = r#"{"total": 2000, "input": 1500, "output": 500, "reasoning": 0}"#;
        let tokens: TokenUsage = serde_json::from_str(json).unwrap();
        assert_eq!(tokens.total, Some(2000));
        assert_eq!(tokens.input, 1500);
        assert_eq!(tokens.output, 500);
        assert_eq!(tokens.reasoning, 0);
    }

    #[test]
    fn test_token_usage_without_total() {
        let json = r#"{"input": 1500, "output": 500}"#;
        let tokens: TokenUsage = serde_json::from_str(json).unwrap();
        assert!(tokens.total.is_none());
        assert_eq!(tokens.input, 1500);
        assert_eq!(tokens.output, 500);
    }

    #[test]
    fn test_message_path_deserialize() {
        let json = r#"{"cwd": "/path/to/dir", "root": "/path"}"#;
        let path: MessagePath = serde_json::from_str(json).unwrap();
        assert_eq!(path.cwd, "/path/to/dir");
        assert_eq!(path.root, "/path");
    }
}
