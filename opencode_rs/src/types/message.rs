//! Message and content part types for opencode_rs.

use serde::{Deserialize, Serialize};

/// A message in a session.
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

/// A content part within a message.
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
    },
    /// Step start marker.
    #[serde(rename = "step-start")]
    StepStart {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Step name.
        name: String,
    },
    /// Step finish marker.
    #[serde(rename = "step-finish")]
    StepFinish {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Step name.
        name: String,
        /// Step completion status.
        #[serde(default)]
        status: Option<String>,
    },
    /// Agent delegation.
    Agent {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Agent name.
        name: String,
    },
    /// Reasoning/thinking content.
    Reasoning {
        /// Part identifier.
        #[serde(default)]
        id: Option<String>,
        /// Reasoning text.
        text: String,
    },
    /// Unknown part type (forward compatibility).
    #[serde(other)]
    Unknown,
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

/// Request to send a prompt to a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRequest {
    /// Content parts to send.
    pub parts: Vec<PromptPart>,
}

/// A content part in a prompt request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PromptPart {
    /// Text content.
    Text {
        /// Text content.
        text: String,
    },
    /// File attachment.
    File {
        /// MIME type.
        mime: String,
        /// File URL.
        url: String,
    },
    /// Agent delegation.
    Agent {
        /// Agent name.
        name: String,
    },
}
