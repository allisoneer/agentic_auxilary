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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlockParam {
    /// Text content block
    Text {
        /// The text content
        text: String,
        /// Optional citations for the text
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<serde_json::Value>>,
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
    /// Tool use block (echoed from assistant response)
    ToolUse {
        /// Tool use ID
        id: String,
        /// Tool name
        name: String,
        /// Tool input as JSON value
        input: serde_json::Value,
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
    /// Thinking block (echoed from assistant response with extended thinking)
    Thinking {
        /// The thinking content
        thinking: String,
        /// The signature for the thinking block
        signature: String,
    },
    /// Redacted thinking block (echoed from assistant response)
    RedactedThinking {
        /// Redacted data
        data: String,
    },
    /// Server tool use block (echoed from assistant response)
    ServerToolUse {
        /// Tool use ID
        id: String,
        /// Tool name (e.g., `web_search`)
        name: String,
        /// Tool input as JSON value
        input: serde_json::Value,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Search result block (from web search)
    SearchResult {
        /// Search result content
        content: Vec<serde_json::Value>,
        /// Source URL
        source: String,
        /// Result title
        title: String,
        /// Optional citations
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<serde_json::Value>,
        /// Optional cache control for prompt caching
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Web search tool result block
    WebSearchToolResult {
        /// ID of the tool use that produced this result
        tool_use_id: String,
        /// Search result content
        content: serde_json::Value,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlock {
    /// Text content block
    Text {
        /// The text content
        text: String,
        /// Optional citations for the text
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<serde_json::Value>>,
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
    /// Thinking block (extended thinking feature)
    Thinking {
        /// The thinking content
        thinking: String,
        /// The signature for the thinking block
        signature: String,
    },
    /// Redacted thinking block
    RedactedThinking {
        /// Redacted data
        data: String,
    },
    /// Server tool use block (e.g., `web_search`)
    ServerToolUse {
        /// Tool use ID
        id: String,
        /// Tool name (e.g., `web_search`)
        name: String,
        /// Tool input as JSON value
        input: serde_json::Value,
    },
    /// Web search tool result block
    WebSearchToolResult {
        /// ID of the tool use that produced this result
        tool_use_id: String,
        /// Search result content
        content: serde_json::Value,
    },
    /// Unknown content block type (forward compatibility)
    #[serde(other)]
    Unknown,
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

// =========================================================================
// Echo Pattern Conversions (TryFrom)
// =========================================================================

/// Error when converting a response `ContentBlock` to a request `ContentBlockParam`
#[derive(Debug, Clone, thiserror::Error)]
pub enum ContentBlockConversionError {
    /// Cannot convert an unknown content block type
    #[error("cannot convert unknown content block to request content block param")]
    UnknownContentBlock,
}

impl TryFrom<&ContentBlock> for ContentBlockParam {
    type Error = ContentBlockConversionError;

    fn try_from(block: &ContentBlock) -> Result<Self, Self::Error> {
        match block {
            ContentBlock::Text { text, citations } => Ok(Self::Text {
                text: text.clone(),
                citations: citations.clone(),
                cache_control: None,
            }),
            ContentBlock::ToolUse { id, name, input } => Ok(Self::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
                cache_control: None,
            }),
            ContentBlock::Thinking {
                thinking,
                signature,
            } => Ok(Self::Thinking {
                thinking: thinking.clone(),
                signature: signature.clone(),
            }),
            ContentBlock::RedactedThinking { data } => {
                Ok(Self::RedactedThinking { data: data.clone() })
            }
            ContentBlock::ServerToolUse { id, name, input } => Ok(Self::ServerToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
                cache_control: None,
            }),
            ContentBlock::WebSearchToolResult {
                tool_use_id,
                content,
            } => Ok(Self::WebSearchToolResult {
                tool_use_id: tool_use_id.clone(),
                content: content.clone(),
                cache_control: None,
            }),
            ContentBlock::Unknown => Err(ContentBlockConversionError::UnknownContentBlock),
        }
    }
}

impl TryFrom<ContentBlock> for ContentBlockParam {
    type Error = ContentBlockConversionError;

    fn try_from(block: ContentBlock) -> Result<Self, Self::Error> {
        // Delegate to the borrowed implementation to avoid code duplication
        Self::try_from(&block)
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
            citations: None,
            cache_control: None,
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"text""#));
        assert!(s.contains(r#""text":"hello""#));
        assert!(!s.contains("cache_control"));
        assert!(!s.contains("citations"));
    }

    #[test]
    fn content_block_response_text_ser() {
        let cb = ContentBlock::Text {
            text: "response".into(),
            citations: None,
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"text""#));
        assert!(s.contains(r#""text":"response""#));
        assert!(!s.contains("citations"));
    }

    #[test]
    fn content_block_unknown_deser() {
        let json = r#"{"type":"future_block","foo":"bar"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(matches!(block, ContentBlock::Unknown));
    }

    #[test]
    fn content_block_param_tool_use_ser() {
        let cb = ContentBlockParam::ToolUse {
            id: "toolu_123".into(),
            name: "get_weather".into(),
            input: serde_json::json!({"city": "Paris"}),
            cache_control: None,
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"tool_use""#));
        assert!(s.contains(r#""id":"toolu_123""#));
        assert!(s.contains(r#""name":"get_weather""#));
    }

    #[test]
    fn content_block_thinking_ser_deser() {
        let cb = ContentBlock::Thinking {
            thinking: "Let me analyze this...".into(),
            signature: "sig_abc123".into(),
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"thinking""#));
        assert!(s.contains(r#""thinking":"Let me analyze this...""#));
        assert!(s.contains(r#""signature":"sig_abc123""#));

        // Deserialize back
        let parsed: ContentBlock = serde_json::from_str(&s).unwrap();
        match parsed {
            ContentBlock::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "Let me analyze this...");
                assert_eq!(signature, "sig_abc123");
            }
            _ => panic!("Expected Thinking variant"),
        }
    }

    #[test]
    fn content_block_param_thinking_ser() {
        let cb = ContentBlockParam::Thinking {
            thinking: "Analyzing...".into(),
            signature: "sig_xyz".into(),
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"thinking""#));
        assert!(s.contains(r#""thinking":"Analyzing...""#));
        assert!(s.contains(r#""signature":"sig_xyz""#));
    }

    #[test]
    fn content_block_redacted_thinking_deser() {
        let json = r#"{"type":"redacted_thinking","data":"redacted_data_here"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::RedactedThinking { data } => {
                assert_eq!(data, "redacted_data_here");
            }
            _ => panic!("Expected RedactedThinking variant"),
        }
    }

    #[test]
    fn content_block_server_tool_use_deser() {
        let json = r#"{"type":"server_tool_use","id":"tool_123","name":"web_search","input":{"query":"rust"}}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::ServerToolUse { id, name, input } => {
                assert_eq!(id, "tool_123");
                assert_eq!(name, "web_search");
                assert_eq!(input["query"], "rust");
            }
            _ => panic!("Expected ServerToolUse variant"),
        }
    }

    #[test]
    fn content_block_web_search_result_deser() {
        let json = r#"{"type":"web_search_tool_result","tool_use_id":"tool_123","content":{"results":[]}}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        match block {
            ContentBlock::WebSearchToolResult {
                tool_use_id,
                content,
            } => {
                assert_eq!(tool_use_id, "tool_123");
                assert!(content.is_object());
            }
            _ => panic!("Expected WebSearchToolResult variant"),
        }
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
            citations: None,
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

    #[test]
    fn content_block_param_search_result_roundtrip() {
        let cb = ContentBlockParam::SearchResult {
            content: vec![serde_json::json!({"text": "search result content"})],
            source: "https://example.com".into(),
            title: "Example Result".into(),
            citations: Some(serde_json::json!({"citation": "data"})),
            cache_control: None,
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"search_result""#));
        assert!(s.contains(r#""source":"https://example.com""#));
        assert!(s.contains(r#""title":"Example Result""#));

        // Round-trip: deserialize back
        let parsed: ContentBlockParam = serde_json::from_str(&s).unwrap();
        match parsed {
            ContentBlockParam::SearchResult {
                source,
                title,
                content,
                ..
            } => {
                assert_eq!(source, "https://example.com");
                assert_eq!(title, "Example Result");
                assert_eq!(content.len(), 1);
            }
            _ => panic!("Expected SearchResult variant"),
        }
    }

    #[test]
    fn content_block_param_web_search_tool_result_roundtrip() {
        let cb = ContentBlockParam::WebSearchToolResult {
            tool_use_id: "toolu_abc".into(),
            content: serde_json::json!({"results": [{"url": "https://example.com"}]}),
            cache_control: None,
        };
        let s = serde_json::to_string(&cb).unwrap();
        assert!(s.contains(r#""type":"web_search_tool_result""#));
        assert!(s.contains(r#""tool_use_id":"toolu_abc""#));

        // Round-trip: deserialize back
        let parsed: ContentBlockParam = serde_json::from_str(&s).unwrap();
        match parsed {
            ContentBlockParam::WebSearchToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "toolu_abc");
                assert!(content["results"].is_array());
            }
            _ => panic!("Expected WebSearchToolResult variant"),
        }
    }

    // =========================================================================
    // TryFrom tests (echo pattern)
    // =========================================================================

    #[test]
    fn tryfrom_content_block_text_to_param() {
        let block = ContentBlock::Text {
            text: "hello world".into(),
            citations: Some(vec![serde_json::json!({"url": "https://example.com"})]),
        };
        let param = ContentBlockParam::try_from(&block).unwrap();
        match param {
            ContentBlockParam::Text {
                text,
                citations,
                cache_control,
            } => {
                assert_eq!(text, "hello world");
                assert!(citations.is_some());
                assert!(cache_control.is_none());
            }
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn tryfrom_content_block_tool_use_to_param() {
        let block = ContentBlock::ToolUse {
            id: "toolu_test".into(),
            name: "get_weather".into(),
            input: serde_json::json!({"city": "Paris"}),
        };
        let param = ContentBlockParam::try_from(&block).unwrap();
        match param {
            ContentBlockParam::ToolUse {
                id,
                name,
                input,
                cache_control,
            } => {
                assert_eq!(id, "toolu_test");
                assert_eq!(name, "get_weather");
                assert_eq!(input["city"], "Paris");
                assert!(cache_control.is_none());
            }
            _ => panic!("Expected ToolUse variant"),
        }
    }

    #[test]
    fn tryfrom_content_block_thinking_to_param() {
        let block = ContentBlock::Thinking {
            thinking: "Let me think...".into(),
            signature: "sig_123".into(),
        };
        let param = ContentBlockParam::try_from(&block).unwrap();
        match param {
            ContentBlockParam::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "Let me think...");
                assert_eq!(signature, "sig_123");
            }
            _ => panic!("Expected Thinking variant"),
        }
    }

    #[test]
    fn tryfrom_content_block_redacted_thinking_to_param() {
        let block = ContentBlock::RedactedThinking {
            data: "redacted".into(),
        };
        let param = ContentBlockParam::try_from(&block).unwrap();
        match param {
            ContentBlockParam::RedactedThinking { data } => {
                assert_eq!(data, "redacted");
            }
            _ => panic!("Expected RedactedThinking variant"),
        }
    }

    #[test]
    fn tryfrom_content_block_server_tool_use_to_param() {
        let block = ContentBlock::ServerToolUse {
            id: "srv_123".into(),
            name: "web_search".into(),
            input: serde_json::json!({"query": "rust"}),
        };
        let param = ContentBlockParam::try_from(&block).unwrap();
        match param {
            ContentBlockParam::ServerToolUse {
                id,
                name,
                input,
                cache_control,
            } => {
                assert_eq!(id, "srv_123");
                assert_eq!(name, "web_search");
                assert_eq!(input["query"], "rust");
                assert!(cache_control.is_none());
            }
            _ => panic!("Expected ServerToolUse variant"),
        }
    }

    #[test]
    fn tryfrom_content_block_web_search_result_to_param() {
        let block = ContentBlock::WebSearchToolResult {
            tool_use_id: "tool_456".into(),
            content: serde_json::json!({"results": []}),
        };
        let param = ContentBlockParam::try_from(&block).unwrap();
        match param {
            ContentBlockParam::WebSearchToolResult {
                tool_use_id,
                content,
                cache_control,
            } => {
                assert_eq!(tool_use_id, "tool_456");
                assert!(content["results"].is_array());
                assert!(cache_control.is_none());
            }
            _ => panic!("Expected WebSearchToolResult variant"),
        }
    }

    #[test]
    fn tryfrom_content_block_unknown_fails() {
        let block = ContentBlock::Unknown;
        let result = ContentBlockParam::try_from(&block);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ContentBlockConversionError::UnknownContentBlock
        ));
    }

    #[test]
    fn tryfrom_content_block_owned_works() {
        let block = ContentBlock::Text {
            text: "owned".into(),
            citations: None,
        };
        // Test the owned version (consumes block)
        let param = ContentBlockParam::try_from(block).unwrap();
        match param {
            ContentBlockParam::Text { text, .. } => {
                assert_eq!(text, "owned");
            }
            _ => panic!("Expected Text variant"),
        }
    }
}
