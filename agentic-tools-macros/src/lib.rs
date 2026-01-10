//! Proc macros for the agentic-tools library family.
//!
//! This crate provides the `#[tool]` attribute macro for defining tools.

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
