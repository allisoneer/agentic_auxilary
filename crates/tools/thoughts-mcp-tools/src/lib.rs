//! Agentic-tools integration for thoughts_tool.
//!
//! This module provides Tool wrappers for the 5 thoughts MCP tools using the
//! agentic-tools-core framework, enabling registration in the unified registry.

pub mod tools;

pub use tools::{
    AddReferenceTool, GetTemplateTool, ListActiveDocumentsTool, ListReferencesTool,
    WriteDocumentTool,
};

use agentic_tools_core::ToolRegistry;

/// Build a ToolRegistry registering all thoughts tools.
///
/// This registry can be merged with other domain registries in Plan 4
/// to create a unified agentic-mcp binary.
pub fn build_registry() -> ToolRegistry {
    ToolRegistry::builder()
        .register::<WriteDocumentTool, ()>(WriteDocumentTool)
        .register::<ListActiveDocumentsTool, ()>(ListActiveDocumentsTool)
        .register::<ListReferencesTool, ()>(ListReferencesTool)
        .register::<AddReferenceTool, ()>(AddReferenceTool)
        .register::<GetTemplateTool, ()>(GetTemplateTool)
        .finish()
}
