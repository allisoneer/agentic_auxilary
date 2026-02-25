use derive_builder::Builder;
use serde::{Deserialize, Serialize};

use super::common::{Metadata, Usage};
use super::content::{
    ContentBlock, ContentBlockConversionError, ContentBlockParam, MessageContentParam,
    MessageParam, MessageRole, SystemParam,
};
use super::tools::{Tool, ToolChoice};

/// Output format for structured outputs (beta)
///
/// Used to constrain the assistant's response to match a specific JSON schema.
/// Requires a structured outputs beta header to be enabled via [`BetaFeature`](crate::BetaFeature).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(clippy::derive_partial_eq_without_eq)] // serde_json::Value doesn't impl Eq
pub enum OutputFormat {
    /// Structured outputs via JSON schema
    #[serde(rename = "json_schema")]
    JsonSchema {
        /// JSON Schema object that the response must conform to
        schema: serde_json::Value,
    },
}

/// Configuration for extended thinking feature
///
/// Controls how Claude uses extended thinking to work through problems step-by-step.
/// Extended thinking allows Claude to reason through complex tasks before providing a response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ThinkingConfig {
    /// Enable extended thinking with a token budget
    Enabled {
        /// Maximum tokens to use for thinking (1024-100000 typically)
        budget_tokens: u32,
    },
    /// Disable extended thinking
    Disabled,
    /// Let the model adaptively choose thinking depth
    Adaptive,
}

/// Output configuration for controlling response generation
///
/// Provides fine-grained control over response format and effort level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[expect(clippy::derive_partial_eq_without_eq)] // OutputFormat contains serde_json::Value
pub struct OutputConfig {
    /// Output format constraint (e.g., JSON schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat>,
    /// Effort level for response generation ("low", "standard", "high")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// Service tier for request routing
///
/// Controls which infrastructure tier processes the request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    /// Automatically select the best available tier
    Auto,
    /// Use only standard tier (no priority/dedicated)
    StandardOnly,
}

/// Request to create a message
#[derive(Debug, Clone, Deserialize, PartialEq, Builder, Default)]
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
    /// Enable streaming responses; set automatically by `create_stream()`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Extended thinking configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Output configuration (format and effort)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// Service tier for request routing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    /// Geographic region hint for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
    /// Deprecated: use `output_config.format` instead
    ///
    /// This field is bridged to `output_config.format` during serialization
    /// for backwards compatibility. If both are set, `output_config` takes precedence.
    #[deprecated(note = "Use `output_config` with `format` field instead")]
    #[serde(skip)]
    pub output_format: Option<OutputFormat>,
}

impl Serialize for MessagesCreateRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        // Count non-None fields for map size hint
        let mut field_count = 2; // model and max_tokens are always present
        field_count += usize::from(self.system.is_some());
        field_count += 1; // messages is always serialized
        field_count += usize::from(self.temperature.is_some());
        field_count += usize::from(self.stop_sequences.is_some());
        field_count += usize::from(self.top_p.is_some());
        field_count += usize::from(self.top_k.is_some());
        field_count += usize::from(self.metadata.is_some());
        field_count += usize::from(self.tools.is_some());
        field_count += usize::from(self.tool_choice.is_some());
        field_count += usize::from(self.stream.is_some());
        field_count += usize::from(self.thinking.is_some());
        field_count += usize::from(self.service_tier.is_some());
        field_count += usize::from(self.inference_geo.is_some());

        // Determine effective output_config: bridge output_format if needed
        #[expect(deprecated)]
        let effective_output_config = if self.output_config.is_some() {
            self.output_config.clone()
        } else if self.output_format.is_some() {
            Some(OutputConfig {
                format: self.output_format.clone(),
                effort: None,
            })
        } else {
            None
        };
        field_count += usize::from(effective_output_config.is_some());

        let mut map = serializer.serialize_map(Some(field_count))?;

        map.serialize_entry("model", &self.model)?;
        map.serialize_entry("max_tokens", &self.max_tokens)?;

        if let Some(ref system) = self.system {
            map.serialize_entry("system", system)?;
        }

        map.serialize_entry("messages", &self.messages)?;

        if let Some(ref temperature) = self.temperature {
            map.serialize_entry("temperature", temperature)?;
        }
        if let Some(ref stop_sequences) = self.stop_sequences {
            map.serialize_entry("stop_sequences", stop_sequences)?;
        }
        if let Some(ref top_p) = self.top_p {
            map.serialize_entry("top_p", top_p)?;
        }
        if let Some(ref top_k) = self.top_k {
            map.serialize_entry("top_k", top_k)?;
        }
        if let Some(ref metadata) = self.metadata {
            map.serialize_entry("metadata", metadata)?;
        }
        if let Some(ref tools) = self.tools {
            map.serialize_entry("tools", tools)?;
        }
        if let Some(ref tool_choice) = self.tool_choice {
            map.serialize_entry("tool_choice", tool_choice)?;
        }
        if let Some(ref stream) = self.stream {
            map.serialize_entry("stream", stream)?;
        }
        if let Some(ref thinking) = self.thinking {
            map.serialize_entry("thinking", thinking)?;
        }
        if let Some(ref output_config) = effective_output_config {
            map.serialize_entry("output_config", output_config)?;
        }
        if let Some(ref service_tier) = self.service_tier {
            map.serialize_entry("service_tier", service_tier)?;
        }
        if let Some(ref inference_geo) = self.inference_geo {
            map.serialize_entry("inference_geo", inference_geo)?;
        }

        map.end()
    }
}

/// Response from creating a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

impl MessagesCreateResponse {
    /// Convert this response into a `MessageParam` for multi-turn conversations.
    ///
    /// This enables the "echo" pattern where the assistant's response is sent back
    /// in a follow-up request (e.g., when handling tool use).
    ///
    /// # Errors
    ///
    /// Returns an error if any content block cannot be converted (e.g., `Unknown` blocks).
    pub fn try_into_message_param(&self) -> Result<MessageParam, ContentBlockConversionError> {
        let blocks = self
            .content
            .iter()
            .map(ContentBlockParam::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(MessageParam {
            role: self.role.clone(),
            content: MessageContentParam::Blocks(blocks),
        })
    }
}

/// Request to count tokens for a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: "Hello".into(),
            }],
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""model":"claude-3-5-sonnet-20241022""#));
        assert!(s.contains(r#""max_tokens":128"#));
        assert!(s.contains(r#""Hello""#));
        // Optional fields should not appear when None
        assert!(!s.contains("stream"));
        assert!(!s.contains("output_format"));
        assert!(!s.contains("output_config"));
        assert!(!s.contains("thinking"));
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
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""system":"You are helpful""#));
    }

    #[test]
    fn message_request_with_blocks() {
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: MessageContentParam::Blocks(vec![ContentBlockParam::Text {
                    text: "Block content".into(),
                    citations: None,
                    cache_control: None,
                }]),
            }],
            temperature: Some(0.7),
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""Block content""#));
        assert!(s.contains(r#""temperature":0.7"#));
    }

    #[test]
    fn thinking_config_enabled_ser() {
        let t = ThinkingConfig::Enabled {
            budget_tokens: 2048,
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""type":"enabled""#));
        assert!(s.contains(r#""budget_tokens":2048"#));
    }

    #[test]
    fn thinking_config_disabled_ser() {
        let t = ThinkingConfig::Disabled;
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""type":"disabled""#));
    }

    #[test]
    fn thinking_config_adaptive_ser() {
        let t = ThinkingConfig::Adaptive;
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""type":"adaptive""#));
    }

    #[test]
    fn service_tier_ser() {
        assert_eq!(
            serde_json::to_string(&ServiceTier::Auto).unwrap(),
            r#""auto""#
        );
        assert_eq!(
            serde_json::to_string(&ServiceTier::StandardOnly).unwrap(),
            r#""standard_only""#
        );
    }

    #[test]
    fn output_config_ser() {
        let oc = OutputConfig {
            format: Some(OutputFormat::JsonSchema {
                schema: serde_json::json!({"type": "object"}),
            }),
            effort: Some("high".into()),
        };
        let s = serde_json::to_string(&oc).unwrap();
        assert!(s.contains(r#""effort":"high""#));
        assert!(s.contains(r#""format""#));
        assert!(s.contains(r#""json_schema""#));
    }

    #[test]
    fn message_request_with_thinking() {
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 16000,
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: "Solve this problem".into(),
            }],
            thinking: Some(ThinkingConfig::Enabled {
                budget_tokens: 4096,
            }),
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""thinking""#));
        assert!(s.contains(r#""type":"enabled""#));
        assert!(s.contains(r#""budget_tokens":4096"#));
    }

    #[test]
    fn message_request_with_service_tier() {
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            messages: vec![],
            service_tier: Some(ServiceTier::Auto),
            inference_geo: Some("us".into()),
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""service_tier":"auto""#));
        assert!(s.contains(r#""inference_geo":"us""#));
    }

    // Tests for deprecated output_format bridge behavior
    // These intentionally use the deprecated field to verify backwards compatibility

    #[test]
    #[expect(deprecated)]
    fn output_format_bridges_to_output_config() {
        // Test that deprecated output_format serializes as output_config.format
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            messages: vec![],
            output_format: Some(OutputFormat::JsonSchema {
                schema: serde_json::json!({"type": "string"}),
            }),
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        // Should serialize as output_config, not output_format
        assert!(!s.contains(r#""output_format""#));
        assert!(s.contains(r#""output_config""#));
        assert!(s.contains(r#""format""#));
        assert!(s.contains(r#""json_schema""#));
    }

    #[test]
    #[expect(deprecated)]
    fn output_config_takes_precedence_over_output_format() {
        // When both are set, output_config wins
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            messages: vec![],
            output_config: Some(OutputConfig {
                format: None,
                effort: Some("high".into()),
            }),
            output_format: Some(OutputFormat::JsonSchema {
                schema: serde_json::json!({"type": "string"}),
            }),
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        // output_config should be used (has effort, no format)
        assert!(s.contains(r#""effort":"high""#));
        // The output_format's json_schema should NOT appear since output_config took precedence
        assert!(!s.contains(r#""json_schema""#));
    }

    #[test]
    fn neither_output_format_nor_output_config_set() {
        let req = MessagesCreateRequest {
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 128,
            messages: vec![],
            ..Default::default()
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(!s.contains("output_config"));
        assert!(!s.contains("output_format"));
    }

    // =========================================================================
    // try_into_message_param tests (echo pattern)
    // =========================================================================

    #[test]
    fn try_into_message_param_with_text() {
        use crate::types::content::ContentBlock;

        let response = MessagesCreateResponse {
            id: "msg_123".into(),
            kind: "message".into(),
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text {
                text: "Hello, world!".into(),
                citations: None,
            }],
            model: "claude-3-5-sonnet-20241022".into(),
            stop_reason: Some("end_turn".into()),
            usage: None,
        };

        let message_param = response.try_into_message_param().unwrap();

        assert_eq!(message_param.role, MessageRole::Assistant);
        match message_param.content {
            MessageContentParam::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlockParam::Text { text, .. } => {
                        assert_eq!(text, "Hello, world!");
                    }
                    _ => panic!("Expected Text block"),
                }
            }
            MessageContentParam::String(_) => panic!("Expected Blocks content"),
        }
    }

    #[test]
    fn try_into_message_param_with_tool_use() {
        use crate::types::content::ContentBlock;

        let response = MessagesCreateResponse {
            id: "msg_456".into(),
            kind: "message".into(),
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Let me check the weather.".into(),
                    citations: None,
                },
                ContentBlock::ToolUse {
                    id: "toolu_abc123".into(),
                    name: "get_weather".into(),
                    input: serde_json::json!({"location": "Paris"}),
                },
            ],
            model: "claude-3-5-sonnet-20241022".into(),
            stop_reason: Some("tool_use".into()),
            usage: None,
        };

        let message_param = response.try_into_message_param().unwrap();

        assert_eq!(message_param.role, MessageRole::Assistant);
        match message_param.content {
            MessageContentParam::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                // Check Text block
                assert!(matches!(&blocks[0], ContentBlockParam::Text { .. }));
                // Check ToolUse block
                match &blocks[1] {
                    ContentBlockParam::ToolUse {
                        id, name, input, ..
                    } => {
                        assert_eq!(id, "toolu_abc123");
                        assert_eq!(name, "get_weather");
                        assert_eq!(input["location"], "Paris");
                    }
                    _ => panic!("Expected ToolUse block"),
                }
            }
            MessageContentParam::String(_) => panic!("Expected Blocks content"),
        }
    }

    #[test]
    fn try_into_message_param_fails_on_unknown_block() {
        use crate::types::content::{ContentBlock, ContentBlockConversionError};

        let response = MessagesCreateResponse {
            id: "msg_789".into(),
            kind: "message".into(),
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Unknown],
            model: "claude-3-5-sonnet-20241022".into(),
            stop_reason: None,
            usage: None,
        };

        let result = response.try_into_message_param();

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ContentBlockConversionError::UnknownContentBlock
        ));
    }
}
