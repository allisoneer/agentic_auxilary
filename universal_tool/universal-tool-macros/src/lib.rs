//! Universal Tool Framework (UTF) Procedural Macros
//!
//! This crate provides the procedural macros that power UTF's code generation.
//! Users should depend on `universal-tool-core`, not this crate directly.
//!
//! See `parser.rs` for detailed documentation on the feature propagation system.

use proc_macro::TokenStream;

mod model;
mod parser;
mod codegen;

/// The main attribute macro for defining universal tools.
/// 
/// This macro will generate interface-specific methods (CLI, REST, MCP) 
/// from your tool implementation.
/// 
/// # Example
/// 
/// ```ignore
/// #[universal_tool_router]
/// impl MyTools {
///     #[universal_tool(description = "Analyze code for quality metrics")]
///     async fn analyze_code(
///         &self,
///         path: String,
///         detailed: Option<bool>
///     ) -> Result<AnalysisResult, ToolError> {
///         // Your business logic here
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn universal_tool_router(attr: TokenStream, item: TokenStream) -> TokenStream {
    parser::parse_router(attr.into(), item.into())
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Attribute macro for individual tool methods within a universal_tool_router impl block.
#[proc_macro_attribute]
pub fn universal_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker macro - the actual parsing happens in universal_tool_router
    // We just pass through the original item unchanged
    item
}

// Note: universal_tool_param is not a proc macro attribute!
// It's an inert helper attribute that gets parsed by universal_tool_router.
// This allows it to be used on function parameters, which Rust doesn't allow
// for arbitrary proc macro attributes.
