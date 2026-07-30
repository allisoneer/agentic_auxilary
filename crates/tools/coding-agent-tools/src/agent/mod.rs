//! Agent spawning module for Claude Code subagents.
//!
//! This module provides configuration and utilities for spawning opinionated
//! Claude Code subagents with specific behaviors based on type and location.

pub mod config;
pub mod evidence;
pub mod guidance;
pub mod prompts;

#[cfg(test)]
mod live_tests;

pub(crate) struct ToolMatrixCell {
    pub(crate) agent_type: crate::types::AgentType,
    pub(crate) location: crate::types::AgentLocation,
    pub(crate) tools: &'static [&'static str],
}

pub(crate) static TOOL_MATRIX: [ToolMatrixCell; 8] = [
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Locator,
        location: crate::types::AgentLocation::Codebase,
        tools: &[
            "mcp__agentic-mcp__cli_ls",
            "mcp__agentic-mcp__cli_grep",
            "mcp__agentic-mcp__cli_glob",
        ],
    },
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Locator,
        location: crate::types::AgentLocation::Thoughts,
        tools: &[
            "mcp__agentic-mcp__cli_ls",
            "mcp__agentic-mcp__thoughts_list_documents",
            "mcp__agentic-mcp__cli_grep",
            "mcp__agentic-mcp__cli_glob",
        ],
    },
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Locator,
        location: crate::types::AgentLocation::References,
        tools: &[
            "mcp__agentic-mcp__cli_ls",
            "mcp__agentic-mcp__thoughts_list_references",
            "mcp__agentic-mcp__cli_grep",
            "mcp__agentic-mcp__cli_glob",
        ],
    },
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Locator,
        location: crate::types::AgentLocation::Web,
        tools: &[
            "mcp__agentic-mcp__web_search",
            "mcp__agentic-mcp__web_fetch",
        ],
    },
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Analyzer,
        location: crate::types::AgentLocation::Codebase,
        tools: &[
            "mcp__agentic-mcp__workspace_read",
            "mcp__agentic-mcp__cli_ls",
            "mcp__agentic-mcp__cli_grep",
            "mcp__agentic-mcp__cli_glob",
            "mcp__agentic-mcp__workspace_todowrite",
        ],
    },
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Analyzer,
        location: crate::types::AgentLocation::Thoughts,
        tools: &[
            "mcp__agentic-mcp__thoughts_read_document",
            "mcp__agentic-mcp__cli_ls",
            "mcp__agentic-mcp__thoughts_list_documents",
            "mcp__agentic-mcp__cli_grep",
            "mcp__agentic-mcp__cli_glob",
        ],
    },
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Analyzer,
        location: crate::types::AgentLocation::References,
        tools: &[
            "mcp__agentic-mcp__thoughts_read_reference",
            "mcp__agentic-mcp__cli_ls",
            "mcp__agentic-mcp__thoughts_list_references",
            "mcp__agentic-mcp__cli_grep",
            "mcp__agentic-mcp__cli_glob",
            "mcp__agentic-mcp__workspace_todowrite",
        ],
    },
    ToolMatrixCell {
        agent_type: crate::types::AgentType::Analyzer,
        location: crate::types::AgentLocation::Web,
        tools: &[
            "mcp__agentic-mcp__web_search",
            "mcp__agentic-mcp__web_fetch",
            "mcp__agentic-mcp__workspace_todowrite",
        ],
    },
];

pub(crate) fn tool_ids_for(
    agent_type: crate::types::AgentType,
    location: crate::types::AgentLocation,
) -> &'static [&'static str] {
    use crate::types::AgentLocation;
    use crate::types::AgentType;

    let cell = match (agent_type, location) {
        (AgentType::Locator, AgentLocation::Codebase) => &TOOL_MATRIX[0],
        (AgentType::Locator, AgentLocation::Thoughts) => &TOOL_MATRIX[1],
        (AgentType::Locator, AgentLocation::References) => &TOOL_MATRIX[2],
        (AgentType::Locator, AgentLocation::Web) => &TOOL_MATRIX[3],
        (AgentType::Analyzer, AgentLocation::Codebase) => &TOOL_MATRIX[4],
        (AgentType::Analyzer, AgentLocation::Thoughts) => &TOOL_MATRIX[5],
        (AgentType::Analyzer, AgentLocation::References) => &TOOL_MATRIX[6],
        (AgentType::Analyzer, AgentLocation::Web) => &TOOL_MATRIX[7],
    };
    debug_assert!(cell.agent_type == agent_type && cell.location == location);
    cell.tools
}

pub use config::build_mcp_config;
pub use config::compose_prompt;
pub use config::enabled_tools_for;
pub use config::model_for;
pub use prompts::ANALYZER_BASE_PROMPT;
pub use prompts::LOCATOR_BASE_PROMPT;
