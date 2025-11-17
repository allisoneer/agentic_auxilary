use serde::{Deserialize, Serialize};

use super::common::{CacheControl, Usage};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    // Future extensibility: ToolUse, ToolResult, Image, Document...
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
}

// Placeholder request/response types for next phase
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessagesCreateRequest {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<ContentBlock>>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessagesCreateResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String, // "message"
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageTokensCountRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<ContentBlock>>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageTokensCountResponse {
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
