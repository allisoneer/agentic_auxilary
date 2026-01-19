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
//!
//! # Automatic TextFormat Detection
//!
//! The registry automatically detects if a tool's output type implements [`TextFormat`]
//! at registration time. If it does, the custom formatter is used; otherwise, output
//! falls back to pretty-printed JSON. This detection uses a type-erased formatter
//! captured at registration time.
//!
//! **Note**: This approach uses monomorphization tricks on stable Rust. If/when
//! `min_specialization` stabilizes, this could be simplified to use trait specialization
//! directly.

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

// ============================================================================
// Type-erased formatter infrastructure
// ============================================================================
//
// This section implements automatic TextFormat detection at registration time.
// The key insight is that we capture a formatting function when the tool is
// registered (while we still know the concrete types), and store it as a
// type-erased function pointer. At call time, we invoke it if present.
//
// This approach works on stable Rust without specialization by using
// monomorphization: we define helper functions with different trait bounds,
// and only the one whose bounds are satisfied gets instantiated.

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

// ----------------------------------------------------------------------------
// Formatter builder machinery
// ----------------------------------------------------------------------------
//
// We use a trait with two non-overlapping implementations to select between
// "has TextFormat" and "no TextFormat" at compile time. The implementations
// are on different marker types, avoiding coherence issues.

/// Internal trait for building formatters. Not part of public API.
pub trait BuildFormatter<W> {
    fn build() -> ErasedFmt;
}

/// Marker type for the fallback (no TextFormat) case.
pub struct FallbackFormatter;

/// Marker type for the TextFormat case.
pub struct TextFormatFormatter;

// Fallback implementation: always returns None
impl<W> BuildFormatter<W> for FallbackFormatter
where
    W: Send + 'static,
{
    fn build() -> ErasedFmt {
        ErasedFmt::none()
    }
}

// TextFormat implementation: captures a function that calls fmt_text
impl<W> BuildFormatter<W> for TextFormatFormatter
where
    W: TextFormat + Send + 'static,
{
    fn build() -> ErasedFmt {
        ErasedFmt {
            fmt_fn: Some(|any, _json, opts| any.downcast_ref::<W>().map(|w| w.fmt_text(opts))),
        }
    }
}

/// Build a formatter for a type that implements [`TextFormat`].
///
/// This is the explicit builder used when you know the type implements `TextFormat`.
pub fn build_formatter_for_textformat<W>() -> ErasedFmt
where
    W: TextFormat + Send + 'static,
{
    ErasedFmt {
        fmt_fn: Some(|any, _json, opts| any.downcast_ref::<W>().map(|w| w.fmt_text(opts))),
    }
}

// ============================================================================
// MakeFormatter trait - the key to auto-detection
// ============================================================================
//
// This trait allows codecs to provide a formatter for their wire output type.
// The identity codec `()` implements this when `T::Output: TextFormat`.
//
// For tools WITHOUT TextFormat, the registry uses `MakeFormatterFallback` instead.
// The registry contains logic to try MakeFormatter first, falling back as needed.
//
// NOTE: If/when Rust stabilizes `min_specialization`, this could be simplified:
// ```rust
// // With specialization (nightly only):
// impl<T, C> MakeFormatter<T> for C where C: ToolCodec<T> {
//     default fn make_formatter() -> ErasedFmt { ErasedFmt::none() }
// }
// impl<T> MakeFormatter<T> for () where T::Output: TextFormat {
//     fn make_formatter() -> ErasedFmt { build_formatter_for_textformat::<T::Output>() }
// }
// ```

/// Trait for codecs to provide a formatter for their wire output type.
///
/// This is implemented for the identity codec `()` when `T::Output: TextFormat`.
/// The registry uses this to capture custom formatters at registration time.
pub trait MakeFormatter<T>
where
    T: crate::tool::Tool,
    Self: crate::tool::ToolCodec<T>,
{
    /// Build a formatter for this codec's wire output type.
    fn make_formatter() -> ErasedFmt;
}

// Implementation for identity codec () when Output implements TextFormat
impl<T> MakeFormatter<T> for ()
where
    T: crate::tool::Tool,
    T::Input: serde::de::DeserializeOwned + schemars::JsonSchema,
    T::Output: serde::Serialize + schemars::JsonSchema + TextFormat + Send + 'static,
{
    fn make_formatter() -> ErasedFmt {
        build_formatter_for_textformat::<T::Output>()
    }
}

/// Fallback trait for codecs - always returns `ErasedFmt::none()`.
///
/// This is always implemented for all codec+tool combinations, providing
/// the JSON fallback formatter. The registry uses this when `MakeFormatter`
/// is not implemented (i.e., when the output type doesn't have `TextFormat`).
pub trait MakeFormatterFallback<T>
where
    T: crate::tool::Tool,
    Self: crate::tool::ToolCodec<T>,
{
    /// Build a fallback formatter (returns None, triggering JSON pretty-print).
    fn make_formatter_fallback() -> ErasedFmt {
        ErasedFmt::none()
    }
}

// Blanket impl: all codecs can provide the fallback
impl<T, C> MakeFormatterFallback<T> for C
where
    T: crate::tool::Tool,
    C: crate::tool::ToolCodec<T>,
{
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
