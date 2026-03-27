//! MCP server integration for the agentic-tools library family.
//!
//! This crate provides [`RegistryServer`], an rmcp-backed server handler
//! that wraps a [`ToolRegistry`] with optional allowlist filtering.

mod server;

pub use server::OutputMode;
pub use server::RegistryServer;

// Re-export rmcp types for convenience
pub use rmcp::ServerHandler;
pub use rmcp::service::ServiceExt;
pub use rmcp::transport::stdio;
