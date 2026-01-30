//! Proc macros for the agentic-tools library family.
//!
//! This crate provides:
//! - `#[tool]` attribute macro for defining tools
//! - `#[derive(TextFormat)]` for implementing the TextFormat trait

mod text_format;
mod tool;

use proc_macro::TokenStream;

/// Attribute macro to define a tool from an async function.
///
/// # Usage
///
/// ```ignore
/// use agentic_tools_macros::tool;
/// use agentic_tools_core::ToolError;
///
/// #[tool(name = "my_tool", description = "Does something useful")]
/// async fn my_tool(input: MyInput) -> Result<MyOutput, ToolError> {
///     // implementation
/// }
/// ```
///
/// This generates a `MyToolTool` struct implementing the `Tool` trait.
///
/// # Attributes
///
/// - `name`: The tool's unique name (defaults to function name)
/// - `description`: Human-readable description of what the tool does
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    tool::expand(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive macro for implementing the TextFormat trait.
///
/// By default, produces pretty-printed JSON. When `markdown = true` in TextOptions,
/// wraps the JSON in a fenced code block.
///
/// # Usage
///
/// ```ignore
/// use agentic_tools_macros::TextFormat;
///
/// #[derive(TextFormat, serde::Serialize)]
/// struct MyOutput {
///     message: String,
///     count: usize,
/// }
/// ```
///
/// # Attributes
///
/// - `#[text_format(with = "path::to_fn")]`: Delegate formatting to a custom function.
///   The function must have signature `fn(&Self, &TextOptions) -> String`.
///
/// ```ignore
/// fn format_my_output(output: &MyOutput, opts: &TextOptions) -> String {
///     format!("Message: {} (count: {})", output.message, output.count)
/// }
///
/// #[derive(TextFormat, serde::Serialize)]
/// #[text_format(with = "format_my_output")]
/// struct MyOutput {
///     message: String,
///     count: usize,
/// }
/// ```
#[proc_macro_derive(TextFormat, attributes(text_format))]
pub fn derive_text_format(input: TokenStream) -> TokenStream {
    text_format::expand(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
