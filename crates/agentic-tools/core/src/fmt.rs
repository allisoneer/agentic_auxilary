//! Transport-agnostic text formatting for tool outputs.
//!
//! This module provides the [`TextFormat`] trait for converting tool outputs to
//! human-readable text, along with supporting types and helpers.
//!
//! # Usage
//!
//! Implement [`TextFormat`] for your tool output types to provide custom formatting:
//!
//! ```ignore
//! use agentic_tools_core::fmt::{TextFormat, TextOptions};
//!
//! struct MyOutput {
//!     count: usize,
//!     items: Vec<String>,
//! }
//!
//! impl TextFormat for MyOutput {
//!     fn fmt_text(&self, opts: &TextOptions) -> String {
//!         format!("Found {} items:\n{}", self.count, self.items.join("\n"))
//!     }
//! }
//! ```
//!
//! For types that don't implement [`TextFormat`], use [`fallback_text_from_json`]
//! to pretty-print the JSON representation.

use crate::tool::{Tool, ToolCodec};
use serde_json::Value as JsonValue;

/// Text rendering style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TextStyle {
    /// Human-friendly formatting with Unicode symbols and formatting.
    #[default]
    Humanized,
    /// Plain text without special formatting.
    Plain,
}

/// Options controlling text formatting behavior.
#[derive(Clone, Debug, Default)]
pub struct TextOptions {
    /// The rendering style to use.
    pub style: TextStyle,
    /// Whether to wrap output in markdown formatting.
    pub markdown: bool,
    /// Maximum number of items to display in collections.
    pub max_items: Option<usize>,
}

impl TextOptions {
    /// Create new text options with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the text style.
    pub fn with_style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Enable or disable markdown formatting.
    pub fn with_markdown(mut self, markdown: bool) -> Self {
        self.markdown = markdown;
        self
    }

    /// Set the maximum number of items to display.
    pub fn with_max_items(mut self, max_items: Option<usize>) -> Self {
        self.max_items = max_items;
        self
    }
}

/// Transport-agnostic text formatting for tool outputs.
///
/// Implement this trait to provide custom human-readable formatting for your
/// tool output types. The formatting is used by both MCP and NAPI servers
/// to produce text alongside JSON data.
pub trait TextFormat {
    /// Format the value as human-readable text.
    ///
    /// The `opts` parameter controls formatting behavior such as style and
    /// markdown wrapping.
    fn fmt_text(&self, opts: &TextOptions) -> String;
}

/// Pretty JSON fallback used when a type does not implement [`TextFormat`].
///
/// This produces a nicely indented JSON string, or falls back to compact
/// JSON if pretty-printing fails.
pub fn fallback_text_from_json(v: &JsonValue) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

/// Codec integration hook for text formatting.
///
/// This trait allows codecs to provide text formatting from their wire output type.
/// A blanket implementation is provided for the identity codec `()` when the
/// wire output type implements [`TextFormat`].
///
/// For custom codecs that want to support text formatting, implement this trait
/// and delegate to the wire output's [`TextFormat`] implementation.
pub trait CodecTextFormatter<T: Tool>: ToolCodec<T> {
    /// Attempt to format the wire output as text.
    ///
    /// Returns `Some(text)` if the codec supports text formatting, `None` otherwise.
    /// The default implementation returns `None`.
    fn format_opt(_wire: &Self::WireOut, _opts: &TextOptions) -> Option<String> {
        None
    }
}

// Blanket implementation: identity codec () provides formatting when WireOut: TextFormat
impl<T> CodecTextFormatter<T> for ()
where
    T: Tool,
    T::Input: serde::de::DeserializeOwned + schemars::JsonSchema,
    T::Output: serde::Serialize + schemars::JsonSchema + TextFormat,
{
    fn format_opt(wire: &Self::WireOut, opts: &TextOptions) -> Option<String> {
        Some(wire.fmt_text(opts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_style_default() {
        let style = TextStyle::default();
        assert_eq!(style, TextStyle::Humanized);
    }

    #[test]
    fn test_text_options_default() {
        let opts = TextOptions::default();
        assert_eq!(opts.style, TextStyle::Humanized);
        assert!(!opts.markdown);
        assert!(opts.max_items.is_none());
    }

    #[test]
    fn test_text_options_builder() {
        let opts = TextOptions::new()
            .with_style(TextStyle::Plain)
            .with_markdown(true)
            .with_max_items(Some(10));

        assert_eq!(opts.style, TextStyle::Plain);
        assert!(opts.markdown);
        assert_eq!(opts.max_items, Some(10));
    }

    #[test]
    fn test_fallback_text_from_json_object() {
        let v = serde_json::json!({"name": "test", "count": 42});
        let text = fallback_text_from_json(&v);
        assert!(text.contains("\"name\": \"test\""));
        assert!(text.contains("\"count\": 42"));
    }

    #[test]
    fn test_fallback_text_from_json_array() {
        let v = serde_json::json!([1, 2, 3]);
        let text = fallback_text_from_json(&v);
        assert!(text.contains("1"));
        assert!(text.contains("2"));
        assert!(text.contains("3"));
    }

    #[test]
    fn test_fallback_text_from_json_null() {
        let v = serde_json::json!(null);
        let text = fallback_text_from_json(&v);
        assert_eq!(text, "null");
    }

    #[test]
    fn test_text_format_impl() {
        struct TestOutput {
            message: String,
        }

        impl TextFormat for TestOutput {
            fn fmt_text(&self, _opts: &TextOptions) -> String {
                format!("Message: {}", self.message)
            }
        }

        let output = TestOutput {
            message: "Hello".to_string(),
        };
        let text = output.fmt_text(&TextOptions::default());
        assert_eq!(text, "Message: Hello");
    }
}
