use serde::{Deserialize, Serialize};

use super::common::CacheControl;

/// Content for tool results
///
/// Can be either a simple string or an array of content blocks.
/// Tool results can contain text or images, but not nested tool results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// Simple string content
    String(String),
    /// Array of content blocks (text or image)
    Blocks(Vec<ToolResultContentBlock>),
}

/// Content block for tool results
///
/// Tool results can contain text or images, but not other tool results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContentBlock {
    /// Text content block
    Text {
        /// The text content
        text: String,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Image content block
    Image {
        /// Image source (base64 or URL)
        source: ImageSource,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

impl From<&str> for ToolResultContent {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for ToolResultContent {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

/// Image source for multimodal content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64-encoded image data
    Base64 {
        /// Media type (e.g., "image/png")
        media_type: String,
        /// Base64-encoded image data
        data: String,
    },
    /// Image URL
    Url {
        /// URL to the image
        url: String,
    },
}

/// Document source for multimodal content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocumentSource {
    /// Base64-encoded document data
    Base64 {
        /// Media type (e.g., "application/pdf")
        media_type: String,
        /// Base64-encoded document data
        data: String,
    },
    /// Document URL
    Url {
        /// URL to the document
        url: String,
    },
}

/// Role of a message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// User message
    User,
    /// Assistant message
    Assistant,
}

// Request-side content blocks
/// Content block parameter for requests
///
/// This enum represents the various types of content that can be sent in a request.
/// Note that this is separate from the response `ContentBlock` enum due to the
/// asymmetric nature of the Anthropic API - requests accept more content types
/// than responses return.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockParam {
    /// Text content block
    Text {
        /// The text content
        text: String,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Tool result block
    ToolResult {
        /// ID of the tool use that this is responding to
        tool_use_id: String,
        /// Optional result content (string or array of content blocks)
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<ToolResultContent>,
        /// Whether this is an error result
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Image content block
    Image {
        /// Image source (base64 or URL)
        source: ImageSource,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Document content block
    Document {
        /// Document source (base64 or URL)
        source: DocumentSource,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

// Response-side content blocks
/// Content block in a response
///
/// This enum represents the various types of content that can be returned in a response.
/// Note that this is separate from the request `ContentBlockParam` enum due to the
/// asymmetric nature of the Anthropic API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content block
    Text {
        /// The text content
        text: String,
    },
    /// Tool use block
    ToolUse {
        /// Tool use ID
        id: String,
        /// Tool name
        name: String,
        /// Tool input as JSON value
        input: serde_json::Value,
    },
}

/// System prompt parameter
///
/// Can be either a simple string or an array of text blocks with cache control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum SystemParam {
    /// Simple string system prompt
    String(String),
    /// Array of text blocks with optional cache control
    Blocks(Vec<TextBlockParam>),
}

/// Message content parameter
///
/// Can be either a simple string or an array of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum MessageContentParam {
    /// Simple string content
    String(String),
    /// Array of content blocks
    Blocks(Vec<ContentBlockParam>),
}

/// Text block parameter for system prompts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextBlockParam {
    /// The text content
    pub text: String,
    /// Type field for serialization (always "text")
    #[serde(
        rename = "type",
        default = "text_type",
        skip_serializing_if = "is_text"
    )]
    pub kind: String,
    /// Optional cache control for prompt caching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

fn text_type() -> String {
    "text".to_string()
}

fn is_text(s: &str) -> bool {
    s == "text"
}

impl TextBlockParam {
    /// Creates a new text block without cache control
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: "text".to_string(),
            cache_control: None,
        }
    }

    /// Creates a new text block with cache control
    #[must_use]
    pub fn with_cache_control(text: impl Into<String>, cache_control: CacheControl) -> Self {
        Self {
            text: text.into(),
            kind: "text".to_string(),
            cache_control: Some(cache_control),
        }
    }
}

/// A message parameter in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageParam {
    /// Role of the message
    pub role: MessageRole,
    /// Content of the message
    pub content: MessageContentParam,
}

// Ergonomic conversions
impl From<&str> for MessageContentParam {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for MessageContentParam {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for SystemParam {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for SystemParam {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn content_block_param_text_ser() {
        let cb = ContentBlockParam::Text {
            text: "hello".into(),
            cache_control: None,
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"text""#));
        assert!(s.contains(r#""text":"hello""#));
        assert!(!s.contains("cache_control"));
    }

    #[test]
    fn content_block_response_text_ser() {
        let cb = ContentBlock::Text {
            text: "response".into(),
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"text""#));
        assert!(s.contains(r#""text":"response""#));
    }

    #[test]
    fn system_param_string() {
        let sys: SystemParam = "You are helpful".into();
        let s = serde_json::to_string(&sys).unwrap();
        assert_eq!(s, r#""You are helpful""#);
    }

    #[test]
    fn system_param_blocks() {
        let sys = SystemParam::Blocks(vec![TextBlockParam::new("test")]);
        let s = serde_json::to_string(&sys).unwrap();
        assert!(s.contains(r#""text":"test""#));
    }

    #[test]
    fn message_content_param_string() {
        let content: MessageContentParam = "hello".into();
        let s = serde_json::to_string(&content).unwrap();
        assert_eq!(s, r#""hello""#);
    }

    #[test]
    fn message_content_param_blocks() {
        let content = MessageContentParam::Blocks(vec![ContentBlockParam::Text {
            text: "test".into(),
            cache_control: None,
        }]);
        let s = serde_json::to_string(&content).unwrap();
        assert!(s.contains(r#""type":"text""#));
        assert!(s.contains(r#""text":"test""#));
    }

    #[test]
    fn text_block_param_with_cache() {
        let tb = TextBlockParam::with_cache_control("cached", CacheControl::ephemeral_1h());
        let s = serde_json::to_string(&tb).unwrap();
        assert!(s.contains(r#""text":"cached""#));
        assert!(s.contains(r#""cache_control""#));
    }
}
