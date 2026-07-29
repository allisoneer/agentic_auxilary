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
pub use tools::ReadDocumentTool;
pub use tools::ReadReferenceTool;
pub use tools::WriteDocumentTool;

#[derive(Debug, Clone, Copy, Default)]
pub struct ThoughtsRuntimeOptions {
    pub read_document: bool,
    pub read_reference: bool,
    pub read_only_nested: bool,
}

use agentic_config::types::ThoughtsConfig;
use agentic_tools_core::ToolRegistry;

/// Build a `ToolRegistry` registering all thoughts tools.
///
/// This registry can be merged with other domain registries in Plan 4
/// to create a unified agentic-mcp binary.
pub fn build_registry(thoughts: ThoughtsConfig) -> ToolRegistry {
    build_registry_with_options(thoughts, ThoughtsRuntimeOptions::default())
}

pub fn build_registry_with_options(
    thoughts: ThoughtsConfig,
    options: ThoughtsRuntimeOptions,
) -> ToolRegistry {
    let readiness = if options.read_only_nested {
        ThoughtsMcpReadinessGate::new_with_check(|| Box::pin(async { Ok(()) }))
    } else {
        ThoughtsMcpReadinessGate::new()
    };

    let mut builder = ToolRegistry::builder()
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
        .register::<GetTemplateTool, ()>(GetTemplateTool {
            readiness: readiness.clone(),
        });
    if options.read_document {
        builder = builder.register::<ReadDocumentTool, ()>(ReadDocumentTool {
            readiness: readiness.clone(),
        });
    }
    if options.read_reference {
        builder = builder.register::<ReadReferenceTool, ()>(ReadReferenceTool { readiness });
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specialized_readers_are_default_disabled_and_independently_gated() {
        let default = build_registry(ThoughtsConfig::default());
        assert!(!default.contains("thoughts_read_document"));
        assert!(!default.contains("thoughts_read_reference"));

        let documents = build_registry_with_options(
            ThoughtsConfig::default(),
            ThoughtsRuntimeOptions {
                read_document: true,
                read_reference: false,
                read_only_nested: true,
            },
        );
        assert!(documents.contains("thoughts_read_document"));
        assert!(!documents.contains("thoughts_read_reference"));
    }
}
