//! MCP server integration for the agentic-tools library family.
//!
//! This crate provides [`RegistryServer`], an rmcp-backed server handler
//! that wraps a [`ToolRegistry`] with optional allowlist filtering.

mod server;

pub use server::{OutputMode, RegistryServer};

// Re-export rmcp types for convenience
pub use rmcp::transport::stdio;
pub use rmcp::{ServerHandler, service::ServiceExt};
