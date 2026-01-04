//! Message and content part types for opencode_rs.

use serde::{Deserialize, Serialize};

/// Message info (metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageInfo {
    /// Unique message identifier.
    pub id: String,
    /// Session ID.
    pub session_id: String,
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

/// A message with its parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageWithParts {
    /// Message info.
    #[serde(flatten)]
    pub info: MessageInfo,
    /// Content parts.
    pub parts: Vec<Part>,
}

/// A message in a session (simplified for list responses).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// Unique message identifier.
    pub id: String,
    /// Message role (user, assistant, system).
    pub role: String,
    /// Content parts of the message.
    pub parts: Vec<Part>,
    /// Model used to generate the message (if assistant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// A content part within a message (12 variants).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        source: Option<serde_json::Value>,
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
        error: Option<serde_json::Value>,
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

/// State of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolState {
    /// Execution status.
    pub status: String,
    /// Tool output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
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
}
