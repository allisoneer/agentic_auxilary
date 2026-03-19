//! Core traits and types for the agentic-tools library family.
//!
//! This crate provides:
//! - [`Tool`] trait: Native-first tool definition with no serde bounds
//! - [`ToolCodec`] trait: Serialization boundary for protocol integration
//! - [`ToolRegistry`]: Type-safe tool storage with native and JSON dispatch
//! - [`SchemaEngine`]: Runtime schema transforms for provider flexibility
//! - [`TextFormat`] trait: Transport-agnostic text formatting for tool outputs
//! - Provider renderers: OpenAI, Anthropic, and MCP schema generation

pub mod context;
pub mod error;
pub mod fmt;
pub mod providers;
pub mod registry;
pub mod schema;
pub mod tool;

pub use context::ToolContext;
pub use error::ToolError;
pub use fmt::ErasedFmt;
pub use fmt::TextFormat;
pub use fmt::TextOptions;
pub use fmt::TextStyle;
pub use fmt::fallback_text_from_json;
pub use registry::FormattedResult;
pub use registry::ToolHandle;
pub use registry::ToolRegistry;
pub use registry::ToolRegistryBuilder;
pub use schema::FieldConstraint;
pub use schema::SchemaEngine;
pub use schema::SchemaTransform;
pub use tool::Tool;
pub use tool::ToolCodec;

// Re-export BoxFuture to support macro-generated signatures without exposing futures crate
pub use futures::future::BoxFuture;
