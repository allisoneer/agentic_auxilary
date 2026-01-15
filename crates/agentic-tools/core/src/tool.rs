//! Core tool traits for native-first tool definitions.

use crate::context::ToolContext;
use crate::error::ToolError;
use futures::future::BoxFuture;

/// Native-first Tool trait with NO serde bounds.
///
/// This trait defines a tool that can be called with native Rust types.
/// Serialization is handled separately by [`ToolCodec`] at protocol boundaries.
///
/// # Example
///
/// ```ignore
/// use agentic_tools_core::{Tool, ToolContext, ToolError};
/// use futures::future::BoxFuture;
///
/// struct GreetTool;
///
/// impl Tool for GreetTool {
///     type Input = String;
///     type Output = String;
///     const NAME: &'static str = "greet";
///     const DESCRIPTION: &'static str = "Greet someone by name";
///
///     fn call(&self, input: Self::Input, _ctx: &ToolContext)
///         -> BoxFuture<'static, Result<Self::Output, ToolError>>
///     {
///         Box::pin(async move {
///             Ok(format!("Hello, {}!", input))
///         })
///     }
/// }
/// ```
pub trait Tool: Send + Sync + 'static {
    /// Input type for the tool (no serde bounds required).
    type Input: Send + 'static;

    /// Output type for the tool (no serde bounds required).
    type Output: Send + 'static;

    /// Unique name identifying the tool.
    const NAME: &'static str;

    /// Human-readable description of what the tool does.
    const DESCRIPTION: &'static str;

    /// Execute the tool with the given input and context.
    fn call(
        &self,
        input: Self::Input,
        ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>>;
}

/// Codec for serializing tool inputs/outputs at protocol boundaries.
///
/// Serde and schemars bounds reside here, NOT on the [`Tool`] trait.
/// This keeps native Rust calls zero-serialization while supporting
/// MCP, napi, and provider schema generation.
///
/// # Identity Codec
///
/// When `T::Input` and `T::Output` already implement the required serde/schemars
/// traits, use `()` as the codec (blanket implementation provided).
pub trait ToolCodec<T: Tool>: Send + Sync + 'static {
    /// Wire format for input (must be deserializable and have JSON schema).
    type WireIn: serde::de::DeserializeOwned + schemars::JsonSchema + Send + 'static;

    /// Wire format for output (must be serializable and have JSON schema).
    type WireOut: serde::Serialize + schemars::JsonSchema + Send + 'static;

    /// Decode wire input to native input.
    fn decode(wire: Self::WireIn) -> Result<T::Input, ToolError>;

    /// Encode native output to wire output.
    fn encode(native: T::Output) -> Result<Self::WireOut, ToolError>;
}

/// Identity codec: when Input/Output already have serde/schemars, use `()` as codec.
impl<T> ToolCodec<T> for ()
where
    T: Tool,
    T::Input: serde::de::DeserializeOwned + schemars::JsonSchema,
    T::Output: serde::Serialize + schemars::JsonSchema,
{
    type WireIn = T::Input;
    type WireOut = T::Output;

    fn decode(wire: Self::WireIn) -> Result<T::Input, ToolError> {
        Ok(wire)
    }

    fn encode(native: T::Output) -> Result<Self::WireOut, ToolError> {
        Ok(native)
    }
}
