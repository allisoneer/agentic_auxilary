use serde::{Deserialize, Serialize};

use super::common::{CacheControl, Usage};

/// Role of a message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// User message
    User,
    /// Assistant message
    Assistant,
}

/// Content block within a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content block
    Text {
        /// The text content
        text: String,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    // Future extensibility: ToolUse, ToolResult, Image, Document...
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    /// Role of the message
    pub role: MessageRole,
    /// Content blocks in the message
    pub content: Vec<ContentBlock>,
}

/// Request to create a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessagesCreateRequest {
    /// Model to use for generation
    pub model: String,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Optional system prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<ContentBlock>>,
    /// Conversation messages
    pub messages: Vec<Message>,
    /// Optional temperature for sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
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
    pub system: Option<Vec<ContentBlock>>,
    /// Conversation messages
    pub messages: Vec<Message>,
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

    #[test]
    fn content_block_text_ser() {
        let cb = ContentBlock::Text {
            text: "hello".into(),
            cache_control: None,
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"text""#));
        assert!(s.contains(r#""text":"hello""#));
        assert!(!s.contains("cache_control"));
    }

    #[test]
    fn message_role_ser() {
        assert_eq!(
            serde_json::to_string(&MessageRole::User).unwrap(),
            r#""user""#
        );
        assert_eq!(
            serde_json::to_string(&MessageRole::Assistant).unwrap(),
            r#""assistant""#
        );
    }
}
