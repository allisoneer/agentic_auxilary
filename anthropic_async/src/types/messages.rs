use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use super::common::{Metadata, Usage};
use super::content::{ContentBlock, MessageParam, MessageRole, SystemParam};
use super::tools::{Tool, ToolChoice};

/// Request to create a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Builder, Default)]
#[builder(setter(into, strip_option), default)]
pub struct MessagesCreateRequest {
    /// Model to use for generation
    #[builder(default)]
    pub model: String,
    /// Maximum tokens to generate
    #[builder(default)]
    pub max_tokens: u32,
    /// Optional system prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemParam>,
    /// Conversation messages
    #[builder(default)]
    pub messages: Vec<MessageParam>,
    /// Optional temperature for sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Optional stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Optional nucleus sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Optional top-k sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Optional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Optional tools for Claude to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Optional tool choice strategy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

/// Response from creating a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessagesCreateResponse {
    /// Message ID
    pub id: String,
    /// Type of response (always "message")
    #[serde(rename = "type")]
    pub kind: String,
    /// Role of the response
    pub role: MessageRole,
    /// Content blocks in the response
    pub content: Vec<ContentBlock>,
    /// Model used for generation
    pub model: String,
    /// Optional stop reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Optional token usage information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// Request to count tokens for a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageTokensCountRequest {
    /// Model to use for token counting
    pub model: String,
    /// Optional system prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemParam>,
    /// Conversation messages
    pub messages: Vec<MessageParam>,
    /// Optional tools for Claude to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Optional tool choice strategy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
}

/// Response from counting tokens
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageTokensCountResponse {
    /// Number of input tokens
    pub input_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::{ContentBlockParam, MessageContentParam};

    #[test]
    fn message_request_ser() {
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            system: None,
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: "Hello".into(),
            }],
            temperature: None,
            stop_sequences: None,
            top_p: None,
            top_k: None,
            metadata: None,
            tools: None,
            tool_choice: None,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""model":"claude-3-5-sonnet-20241022""#));
        assert!(s.contains(r#""max_tokens":128"#));
        assert!(s.contains(r#""Hello""#));
    }

    #[test]
    fn message_request_with_system_string() {
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            system: Some("You are helpful".into()),
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: "Hello".into(),
            }],
            temperature: None,
            stop_sequences: None,
            top_p: None,
            top_k: None,
            metadata: None,
            tools: None,
            tool_choice: None,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""system":"You are helpful""#));
    }

    #[test]
    fn message_request_with_blocks() {
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            system: None,
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: MessageContentParam::Blocks(vec![ContentBlockParam::Text {
                    text: "Block content".into(),
                    cache_control: None,
                }]),
            }],
            temperature: Some(0.7),
            stop_sequences: None,
            top_p: None,
            top_k: None,
            metadata: None,
            tools: None,
            tool_choice: None,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""Block content""#));
        assert!(s.contains(r#""temperature":0.7"#));
    }
}
