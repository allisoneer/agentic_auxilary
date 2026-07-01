//! Agentic-tools integration for `thoughts_tool`.
//!
//! This module provides Tool wrappers for the 6 thoughts MCP tools using the
//! agentic-tools-core framework, enabling registration in the unified registry.

pub(crate) mod readiness;
pub mod tools;

use readiness::ThoughtsMcpReadinessGate;

pub use tools::AddReferenceTool;
pub use tools::GetRepoRefsTool;
pub use tools::GetTemplateTool;
pub use tools::ListActiveDocumentsTool;
pub use tools::ListReferencesTool;
pub use tools::WriteDocumentTool;

use agentic_config::types::ThoughtsConfig;
use agentic_tools_core::ToolRegistry;

/// Build a `ToolRegistry` registering all thoughts tools.
///
/// This registry can be merged with other domain registries in Plan 4
/// to create a unified agentic-mcp binary.
pub fn build_registry(thoughts: ThoughtsConfig) -> ToolRegistry {
    let readiness = ThoughtsMcpReadinessGate::new();

    ToolRegistry::builder()
        .register::<WriteDocumentTool, ()>(WriteDocumentTool {
            readiness: readiness.clone(),
        })
        .register::<ListActiveDocumentsTool, ()>(ListActiveDocumentsTool {
            readiness: readiness.clone(),
        })
        .register::<ListReferencesTool, ()>(ListReferencesTool {
            readiness: readiness.clone(),
        })
        .register::<GetRepoRefsTool, ()>(GetRepoRefsTool {
            readiness: readiness.clone(),
        })
        .register::<AddReferenceTool, ()>(AddReferenceTool {
            thoughts,
            readiness: readiness.clone(),
        })
        .register::<GetTemplateTool, ()>(GetTemplateTool { readiness })
        .finish()
}
