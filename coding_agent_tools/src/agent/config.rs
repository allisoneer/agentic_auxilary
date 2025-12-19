//! Mapping utilities for agent configuration.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use claudecode::config::{MCPConfig, MCPServer};
use claudecode::types::Model;
use universal_tool_core::prelude::ToolError;

use super::prompts::compose_prompt_impl;
use crate::types::{AgentLocation, AgentType};

/// Get the model for a given agent type.
/// - Locator → Haiku (fast, cheap)
/// - Analyzer → Sonnet (deep understanding)
pub fn model_for(agent_type: AgentType) -> Model {
    match agent_type {
        AgentType::Locator => Model::Haiku,
        AgentType::Analyzer => Model::Sonnet,
    }
}

/// Public constant listing all MCP tool IDs exported by the 'coding-agent-tools' server.
///
/// NOTE: We only ever disallow tools from our own server here; tools from other
/// MCP servers (e.g., 'thoughts_tool') are not blocked by this module.
pub const CODING_AGENT_TOOLS_MCP: &[&str] = &[
    "mcp__coding-agent-tools__ls",
    "mcp__coding-agent-tools__spawn_agent",
    "mcp__coding-agent-tools__search_grep",
    "mcp__coding-agent-tools__search_glob",
];

//TODO(0): Is this the best way to manage this list?
/// Get the enabled tools for a given type × location combination.
/// This list includes both built-in tools and MCP tools (prefixed with "mcp__").
pub fn enabled_tools_for(agent_type: AgentType, location: AgentLocation) -> Vec<String> {
    use AgentLocation::*;
    use AgentType::*;

    match (agent_type, location) {
        (Locator, Codebase) => vec![
            "mcp__coding-agent-tools__ls".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, Thoughts) => vec![
            "mcp__coding-agent-tools__ls".into(),
            "mcp__thoughts__list_active_documents".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, References) => vec![
            "mcp__coding-agent-tools__ls".into(),
            "mcp__thoughts__list_references".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, Web) => vec!["WebSearch".into(), "WebFetch".into()],
        (Analyzer, Codebase) => vec![
            "Read".into(),
            "mcp__coding-agent-tools__ls".into(),
            "Grep".into(),
            "Glob".into(),
            "TodoWrite".into(),
        ],
        (Analyzer, Thoughts) => vec![
            "Read".into(),
            "mcp__thoughts__list_active_documents".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Analyzer, References) => vec![
            "Read".into(),
            "mcp__thoughts__list_references".into(),
            "Grep".into(),
            "Glob".into(),
            "TodoWrite".into(),
        ],
        (Analyzer, Web) => vec![
            "WebSearch".into(),
            "WebFetch".into(),
            "TodoWrite".into(),
            "Read".into(),
            "Grep".into(),
            "Glob".into(),
            "mcp__coding-agent-tools__ls".into(),
        ],
    }
}

/// Compute MCP tools to disallow for a given enabled tool list and location.
///
/// Behavior:
/// - Only considers our own 'coding-agent-tools' server tools (see CODING_AGENT_TOOLS_MCP)
/// - Returns all server tools that are NOT present in the enabled list
///   (defense-in-depth and ensures 'ls' is only visible when explicitly enabled)
/// - Ignores other MCP servers' tools (e.g., thoughts_tool)
pub fn disallowed_mcp_tools_for(enabled: &[String], _location: AgentLocation) -> Vec<String> {
    use std::collections::HashSet;

    let enabled_set: HashSet<&str> = enabled.iter().map(|s| s.as_str()).collect();

    CODING_AGENT_TOOLS_MCP
        .iter()
        .filter(|tool| !enabled_set.contains(*tool))
        .map(|s| (*s).to_string())
        .collect()
}

/// Compose the system prompt for a given type × location combination.
pub fn compose_prompt(agent_type: AgentType, location: AgentLocation) -> String {
    compose_prompt_impl(agent_type, location)
}

//TODO(0): I don't think I really need to modify location? I think we can spawn all at root level
//and let them go. Just with proper instructions for where they should look and such.
/// Resolve the working directory for a given location.
/// Returns None for Web (no working directory needed).
pub fn resolve_working_dir(location: AgentLocation) -> Result<Option<PathBuf>, ToolError> {
    match location {
        AgentLocation::Codebase => {
            let cwd = env::current_dir().map_err(|e| {
                ToolError::internal(format!("Failed to resolve current working directory: {e}"))
            })?;
            Ok(Some(cwd))
        }
        AgentLocation::Thoughts => {
            let base = env::var("THOUGHTS_BASE").unwrap_or_else(|_| "./context".to_string());
            let path = PathBuf::from(base);
            if !path.exists() {
                return Err(ToolError::invalid_input(format!(
                    "Thoughts base directory does not exist: {}",
                    path.display()
                )));
            }
            Ok(Some(path))
        }
        AgentLocation::References => {
            let base = env::var("REFERENCES_BASE").unwrap_or_else(|_| "./references".to_string());
            let path = PathBuf::from(base);
            if !path.exists() {
                return Err(ToolError::invalid_input(format!(
                    "References base directory does not exist: {}",
                    path.display()
                )));
            }
            Ok(Some(path))
        }
        AgentLocation::Web => Ok(None),
    }
}

// NOTE: Binary existence checks (bin_in_path, require_binaries_for_location) have been removed.
// MCP server validation now happens via claudecode::mcp::validate in spawn_agent, which provides
// better error messages with stderr capture and actual handshake verification.

/// Map enabled MCP tools to CLI flags for the coding-agent-tools server.
/// Returns flags like `["--ls"]` when `mcp__coding-agent-tools__ls` is in the enabled list.
fn coding_agent_tools_flags(enabled: &[String]) -> Vec<String> {
    use std::collections::HashSet;

    let set: HashSet<&str> = enabled.iter().map(|s| s.as_str()).collect();
    let mut flags: Vec<String> = Vec::new();

    if set.contains("mcp__coding-agent-tools__ls") {
        flags.push("--ls".to_string());
    }
    if set.contains("mcp__coding-agent-tools__spawn_agent") {
        flags.push("--spawn_agent".to_string());
    }
    if set.contains("mcp__coding-agent-tools__search_grep") {
        flags.push("--search_grep".to_string());
    }
    if set.contains("mcp__coding-agent-tools__search_glob") {
        flags.push("--search_glob".to_string());
    }
    flags
}

/// Build MCP server configuration for a given location, with tool flags.
pub fn build_mcp_config(location: AgentLocation, enabled_tools: &[String]) -> MCPConfig {
    let mut servers: HashMap<String, MCPServer> = HashMap::new();

    // Always include coding-agent-tools server, with tool flags derived from enabled_tools
    let mut args = vec!["mcp".to_string()];
    args.append(&mut coding_agent_tools_flags(enabled_tools));

    servers.insert(
        "coding-agent-tools".to_string(),
        MCPServer::stdio("coding-agent-tools", args),
    );

    // Include thoughts server when needed
    if matches!(
        location,
        AgentLocation::Thoughts | AgentLocation::References
    ) {
        servers.insert(
            "thoughts".to_string(),
            MCPServer::stdio("thoughts", vec!["mcp".to_string()]),
        );
    }

    MCPConfig {
        mcp_servers: servers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_for_locator() {
        assert_eq!(model_for(AgentType::Locator), Model::Haiku);
    }

    #[test]
    fn test_model_for_analyzer() {
        assert_eq!(model_for(AgentType::Analyzer), Model::Sonnet);
    }

    #[test]
    fn test_enabled_tools_locator_codebase() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        assert!(tools.contains(&"mcp__coding-agent-tools__ls".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
        assert!(!tools.contains(&"Read".to_string())); // Locator doesn't read deeply
    }

    #[test]
    fn test_enabled_tools_analyzer_codebase() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        assert!(tools.contains(&"Read".to_string())); // Analyzer can read
        assert!(tools.contains(&"mcp__coding-agent-tools__ls".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
    }

    #[test]
    fn test_enabled_tools_locator_thoughts() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        assert!(tools.contains(&"mcp__thoughts__list_active_documents".to_string()));
        assert!(!tools.contains(&"mcp__coding-agent-tools__ls".to_string()));
    }

    #[test]
    fn test_enabled_tools_locator_references() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::References);
        assert!(tools.contains(&"mcp__thoughts__list_references".to_string()));
    }

    #[test]
    fn test_enabled_tools_locator_web() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::Web);
        assert_eq!(tools, vec!["WebSearch".to_string()]);
    }

    #[test]
    fn test_enabled_tools_analyzer_web() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        assert!(tools.contains(&"WebSearch".to_string()));
        assert!(tools.contains(&"WebFetch".to_string()));
    }

    #[test]
    fn test_enabled_tools_analyzer_web_full_set() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let expected = [
            "WebSearch",
            "WebFetch",
            "TodoWrite",
            "Read",
            "Grep",
            "Glob",
            "mcp__coding-agent-tools__ls",
        ];
        for t in expected {
            assert!(tools.contains(&t.to_string()), "missing tool: {t}");
        }
        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn test_disallowed_mcp_tools_locator_codebase() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let disallowed = disallowed_mcp_tools_for(&enabled, AgentLocation::Codebase);

        // Should disallow recursion/search tools but not ls
        assert!(disallowed.contains(&"mcp__coding-agent-tools__spawn_agent".to_string()));
        assert!(disallowed.contains(&"mcp__coding-agent-tools__search_grep".to_string()));
        assert!(disallowed.contains(&"mcp__coding-agent-tools__search_glob".to_string()));
        assert!(!disallowed.contains(&"mcp__coding-agent-tools__ls".to_string()));
    }

    #[test]
    fn test_disallowed_mcp_tools_locator_thoughts_blocks_all_coding_agent_tools() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let disallowed = disallowed_mcp_tools_for(&enabled, AgentLocation::Thoughts);

        // Since 'ls' is not enabled at Thoughts location, all coding-agent-tools MCP should be disallowed
        for t in CODING_AGENT_TOOLS_MCP {
            assert!(
                disallowed.contains(&t.to_string()),
                "missing in disallowed: {t}"
            );
        }
        assert_eq!(disallowed.len(), CODING_AGENT_TOOLS_MCP.len());
    }

    #[test]
    fn test_disallowed_mcp_tools_analyzer_web_allows_ls() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let disallowed = disallowed_mcp_tools_for(&enabled, AgentLocation::Web);

        // Analyzer+Web has ls enabled, so only spawn_agent/search_* should be disallowed
        assert!(disallowed.contains(&"mcp__coding-agent-tools__spawn_agent".to_string()));
        assert!(disallowed.contains(&"mcp__coding-agent-tools__search_grep".to_string()));
        assert!(disallowed.contains(&"mcp__coding-agent-tools__search_glob".to_string()));
        assert!(!disallowed.contains(&"mcp__coding-agent-tools__ls".to_string()));
        assert_eq!(disallowed.len(), 3);
    }

    #[test]
    fn test_compose_prompt_locator_codebase() {
        let prompt = compose_prompt(AgentType::Locator, AgentLocation::Codebase);
        assert!(prompt.contains("finding WHERE"));
        assert!(prompt.contains("Local codebase"));
    }

    #[test]
    fn test_compose_prompt_analyzer_web() {
        let prompt = compose_prompt(AgentType::Analyzer, AgentLocation::Web);
        assert!(prompt.contains("understanding HOW"));
        assert!(prompt.contains("WebFetch"));
    }

    #[test]
    fn test_resolve_working_dir_codebase() {
        let result = resolve_working_dir(AgentLocation::Codebase);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.is_some());
    }

    #[test]
    fn test_resolve_working_dir_web() {
        let result = resolve_working_dir(AgentLocation::Web);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_build_mcp_config_codebase() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let config = build_mcp_config(AgentLocation::Codebase, &enabled);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(!config.mcp_servers.contains_key("thoughts"));
    }

    #[test]
    fn test_build_mcp_config_thoughts() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let config = build_mcp_config(AgentLocation::Thoughts, &enabled);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(config.mcp_servers.contains_key("thoughts"));
    }

    #[test]
    fn test_build_mcp_config_references() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::References);
        let config = build_mcp_config(AgentLocation::References, &enabled);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(config.mcp_servers.contains_key("thoughts"));
    }

    #[test]
    fn test_build_mcp_config_web() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let config = build_mcp_config(AgentLocation::Web, &enabled);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(!config.mcp_servers.contains_key("thoughts"));
    }

    #[test]
    fn test_coding_agent_tools_flags_locator_codebase() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let flags = coding_agent_tools_flags(&enabled);
        // Locator+Codebase has mcp__coding-agent-tools__ls enabled
        assert!(flags.contains(&"--ls".to_string()));
        assert!(!flags.contains(&"--spawn_agent".to_string()));
        assert!(!flags.contains(&"--search_grep".to_string()));
        assert!(!flags.contains(&"--search_glob".to_string()));
    }

    #[test]
    fn test_coding_agent_tools_flags_analyzer_codebase() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        let flags = coding_agent_tools_flags(&enabled);
        // Analyzer+Codebase has mcp__coding-agent-tools__ls enabled
        assert!(flags.contains(&"--ls".to_string()));
        assert!(!flags.contains(&"--spawn_agent".to_string()));
    }

    #[test]
    fn test_coding_agent_tools_flags_no_mcp_tools() {
        // When no coding-agent-tools MCP tools are enabled, flags should be empty
        let enabled = vec!["Grep".to_string(), "Glob".to_string()];
        let flags = coding_agent_tools_flags(&enabled);
        assert!(flags.is_empty());
    }

    #[test]
    fn test_coding_agent_tools_flags_locator_thoughts() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let flags = coding_agent_tools_flags(&enabled);
        // Locator+Thoughts has no coding-agent-tools MCP tools
        assert!(flags.is_empty());
    }

    #[test]
    fn test_coding_agent_tools_flags_analyzer_web() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let flags = coding_agent_tools_flags(&enabled);
        // Analyzer+Web has mcp__coding-agent-tools__ls enabled
        assert!(flags.contains(&"--ls".to_string()));
        assert_eq!(flags.len(), 1);
    }

    // Test all 8 type×location combinations have valid tools
    #[test]
    fn test_all_combinations_have_tools() {
        for agent_type in [AgentType::Locator, AgentType::Analyzer] {
            for location in [
                AgentLocation::Codebase,
                AgentLocation::Thoughts,
                AgentLocation::References,
                AgentLocation::Web,
            ] {
                let tools = enabled_tools_for(agent_type, location);
                assert!(
                    !tools.is_empty(),
                    "No tools for {:?} + {:?}",
                    agent_type,
                    location
                );
            }
        }
    }

    // Test all 8 type×location combinations have valid prompts
    #[test]
    fn test_all_combinations_have_prompts() {
        for agent_type in [AgentType::Locator, AgentType::Analyzer] {
            for location in [
                AgentLocation::Codebase,
                AgentLocation::Thoughts,
                AgentLocation::References,
                AgentLocation::Web,
            ] {
                let prompt = compose_prompt(agent_type, location);
                assert!(
                    !prompt.is_empty(),
                    "Empty prompt for {:?} + {:?}",
                    agent_type,
                    location
                );
                assert!(
                    prompt.len() > 100,
                    "Prompt too short for {:?} + {:?}",
                    agent_type,
                    location
                );
            }
        }
    }
}
