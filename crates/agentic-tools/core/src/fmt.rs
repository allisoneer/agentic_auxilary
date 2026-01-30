//! Transport-agnostic text formatting for tool outputs.
//!
//! This module provides the [`TextFormat`] trait for converting tool outputs to
//! human-readable text, along with supporting types and helpers.
//!
//! # Usage
//!
//! Tool outputs must implement [`TextFormat`]. The trait provides a default
//! implementation that returns pretty-printed JSON via the `Serialize` supertrait.
//! Types can override `fmt_text` for custom human-friendly formatting:
//!
//! ```ignore
//! use agentic_tools_core::fmt::{TextFormat, TextOptions};
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct MyOutput {
//!     count: usize,
//!     items: Vec<String>,
//! }
//!
//! // Use default (pretty JSON):
//! impl TextFormat for MyOutput {}
//!
//! // Or provide custom formatting:
//! impl TextFormat for MyOutput {
//!     fn fmt_text(&self, opts: &TextOptions) -> String {
//!         format!("Found {} items:\n{}", self.count, self.items.join("\n"))
//!     }
//! }
//! ```
//!
//! # Default TextFormat Fallback
//!
//! The [`TextFormat`] trait requires `Serialize` and provides a default `fmt_text`
//! implementation that produces pretty-printed JSON. This means:
//!
//! - Types with custom formatting override `fmt_text()`
//! - Types wanting JSON fallback use an empty impl: `impl TextFormat for T {}`
//! - The registry always calls `fmt_text()` on the native output—no detection needed

use serde_json::Value as JsonValue;
use std::any::Any;

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
///
/// The default implementation returns pretty-printed JSON. Types can override
/// `fmt_text` to provide custom human-friendly formatting.
pub trait TextFormat: serde::Serialize {
    /// Format the value as human-readable text.
    ///
    /// Default: pretty-printed JSON. Types can override to provide custom
    /// human-friendly formatting.
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Pretty JSON fallback used when a type does not implement [`TextFormat`].
///
/// This produces a nicely indented JSON string, or falls back to compact
/// JSON if pretty-printing fails.
pub fn fallback_text_from_json(v: &JsonValue) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

/// Identity formatter for String: returns the raw string without JSON quoting.
impl TextFormat for String {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        self.clone()
    }
}

// ============================================================================
// Type-erased formatter infrastructure (compatibility API)
// ============================================================================
//
// This infrastructure is preserved for compatibility with external crates.
// The registry now calls `TextFormat::fmt_text()` directly on tool outputs,
// so this type-erased machinery is no longer used internally.

/// Type-erased formatter function signature.
///
/// Takes a reference to the wire output (as `&dyn Any`), the JSON data (for fallback),
/// and formatting options. Returns `Some(text)` if formatting succeeded.
type ErasedFmtFn = fn(&dyn Any, &JsonValue, &TextOptions) -> Option<String>;

/// Type-erased formatter captured at tool registration time.
///
/// This stores an optional formatting function that will be called at runtime
/// to produce human-readable text from tool output. If `None`, the registry
/// falls back to pretty-printed JSON.
#[derive(Clone, Copy)]
pub struct ErasedFmt {
    fmt_fn: Option<ErasedFmtFn>,
}

impl ErasedFmt {
    /// Create an empty formatter (will use JSON fallback).
    pub const fn none() -> Self {
        Self { fmt_fn: None }
    }

    /// Attempt to format the given wire output.
    ///
    /// Returns `Some(text)` if this formatter has a function and it succeeded,
    /// `None` otherwise (caller should use JSON fallback).
    pub fn format(
        &self,
        wire_any: &dyn Any,
        data: &JsonValue,
        opts: &TextOptions,
    ) -> Option<String> {
        self.fmt_fn.and_then(|f| f(wire_any, data, opts))
    }
}

impl std::fmt::Debug for ErasedFmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErasedFmt")
            .field("has_formatter", &self.fmt_fn.is_some())
            .finish()
    }
}

/// Build a formatter for a type that implements [`TextFormat`].
///
/// This is the explicit builder used when you know the type implements `TextFormat`.
/// Kept for compatibility with external crates that may use the `ErasedFmt` API.
pub fn build_formatter_for_textformat<W>() -> ErasedFmt
where
    W: TextFormat + Send + 'static,
{
    ErasedFmt {
        fmt_fn: Some(|any, _json, opts| any.downcast_ref::<W>().map(|w| w.fmt_text(opts))),
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
        #[derive(serde::Serialize)]
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
