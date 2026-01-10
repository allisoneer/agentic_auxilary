//! Core traits and types for the agentic-tools library family.
//!
//! This crate provides:
//! - [`Tool`] trait: Native-first tool definition with no serde bounds
//! - [`ToolCodec`] trait: Serialization boundary for protocol integration
//! - [`ToolRegistry`]: Type-safe tool storage with native and JSON dispatch
//! - [`SchemaEngine`]: Runtime schema transforms for provider flexibility
//! - Provider renderers: OpenAI, Anthropic, and MCP schema generation

pub mod context;
pub mod error;
pub mod providers;
pub mod registry;
pub mod schema;
pub mod tool;

pub use context::ToolContext;
pub use error::ToolError;
pub use registry::{ToolHandle, ToolRegistry, ToolRegistryBuilder};
pub use schema::{FieldConstraint, SchemaEngine, SchemaTransform};
pub use tool::{Tool, ToolCodec};
