//! Mapping utilities for agent configuration.

use std::collections::HashMap;

use claudecode::config::{MCPConfig, MCPServer};
use claudecode::types::Model;

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

/// Public constant listing all MCP tool IDs exported by the 'agentic-mcp' server.
///
/// NOTE: We only ever disallow tools from our own server here; these are the
/// tools that subagents can use from the agentic-mcp server.
pub const AGENTIC_MCP_TOOLS: &[&str] = &[
    "mcp__agentic-mcp__cli_ls",
    "mcp__agentic-mcp__ask_agent",
    "mcp__agentic-mcp__cli_grep",
    "mcp__agentic-mcp__cli_glob",
];

// TODO(2): Intentional explicit match for clarity and compile-time exhaustiveness.
// We keep the hardcoded mapping to avoid accidental tool exposure and ensure deterministic tests.
/// Get the enabled tools for a given type × location combination.
/// This list includes both built-in tools and MCP tools (prefixed with "mcp__").
pub fn enabled_tools_for(agent_type: AgentType, location: AgentLocation) -> Vec<String> {
    use AgentLocation::*;
    use AgentType::*;

    match (agent_type, location) {
        (Locator, Codebase) => vec![
            "mcp__agentic-mcp__cli_ls".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, Thoughts) => vec![
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_documents".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, References) => vec![
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_references".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, Web) => vec!["WebSearch".into(), "WebFetch".into()],
        (Analyzer, Codebase) => vec![
            "Read".into(),
            "mcp__agentic-mcp__cli_ls".into(),
            "Grep".into(),
            "Glob".into(),
            "TodoWrite".into(),
        ],
        (Analyzer, Thoughts) => vec![
            "Read".into(),
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_documents".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Analyzer, References) => vec![
            "Read".into(),
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_references".into(),
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
            "mcp__agentic-mcp__cli_ls".into(),
        ],
    }
}

/// Compute MCP tools to disallow for a given enabled tool list and location.
///
/// Behavior:
/// - Only considers our own 'agentic-mcp' server tools (see AGENTIC_MCP_TOOLS)
/// - Returns all server tools that are NOT present in the enabled list
///   (defense-in-depth and ensures 'cli_ls' is only visible when explicitly enabled)
/// - Ignores other MCP servers' tools
pub fn disallowed_mcp_tools_for(enabled: &[String], _location: AgentLocation) -> Vec<String> {
    use std::collections::HashSet;

    let enabled_set: HashSet<&str> = enabled.iter().map(|s| s.as_str()).collect();

    AGENTIC_MCP_TOOLS
        .iter()
        .filter(|tool| !enabled_set.contains(*tool))
        .map(|s| (*s).to_string())
        .collect()
}

/// Compose the system prompt for a given type × location combination.
pub fn compose_prompt(agent_type: AgentType, location: AgentLocation) -> String {
    compose_prompt_impl(agent_type, location)
}

// NOTE: Binary existence checks (bin_in_path, require_binaries_for_location) have been removed.
// MCP server validation now happens via claudecode::mcp::validate in spawn_agent, which provides
// better error messages with stderr capture and actual handshake verification.

/// Map enabled MCP tools to CLI flags for the agentic-mcp server.
/// Returns flags like `["--cli_ls"]` when `mcp__agentic-mcp__cli_ls` is in the enabled list.
fn agentic_mcp_flags(enabled: &[String]) -> Vec<String> {
    use std::collections::HashSet;

    let set: HashSet<&str> = enabled.iter().map(|s| s.as_str()).collect();
    let mut flags: Vec<String> = Vec::new();

    if set.contains("mcp__agentic-mcp__cli_ls") {
        flags.push("--cli_ls".to_string());
    }
    if set.contains("mcp__agentic-mcp__ask_agent") {
        flags.push("--ask_agent".to_string());
    }
    if set.contains("mcp__agentic-mcp__cli_grep") {
        flags.push("--cli_grep".to_string());
    }
    if set.contains("mcp__agentic-mcp__cli_glob") {
        flags.push("--cli_glob".to_string());
    }
    flags
}

/// Build MCP server configuration for a given location, with tool flags.
/// Now uses single agentic-mcp server for all locations.
pub fn build_mcp_config(_location: AgentLocation, enabled_tools: &[String]) -> MCPConfig {
    let mut servers: HashMap<String, MCPServer> = HashMap::new();

    // Single agentic-mcp server with tool flags derived from enabled_tools
    let mut args = vec!["mcp".to_string()];
    args.append(&mut agentic_mcp_flags(enabled_tools));

    servers.insert(
        "agentic-mcp".to_string(),
        MCPServer::stdio("agentic-mcp", args),
    );

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
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
        assert!(!tools.contains(&"Read".to_string())); // Locator doesn't read deeply
    }

    #[test]
    fn test_enabled_tools_analyzer_codebase() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        assert!(tools.contains(&"Read".to_string())); // Analyzer can read
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
    }

    #[test]
    fn test_enabled_tools_locator_thoughts() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        assert!(tools.contains(&"mcp__agentic-mcp__thoughts_list_documents".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
    }

    #[test]
    fn test_enabled_tools_locator_references() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::References);
        assert!(tools.contains(&"mcp__agentic-mcp__thoughts_list_references".to_string()));
    }

    #[test]
    fn test_enabled_tools_analyzer_thoughts_has_ls() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Thoughts);
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__thoughts_list_documents".to_string()));
        assert!(tools.contains(&"Read".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
    }

    #[test]
    fn test_enabled_tools_analyzer_references_has_ls() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::References);
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__thoughts_list_references".to_string()));
        assert!(tools.contains(&"Read".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
        assert!(tools.contains(&"TodoWrite".to_string()));
    }

    #[test]
    fn test_enabled_tools_locator_web() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::Web);
        assert_eq!(tools, vec!["WebSearch".to_string(), "WebFetch".to_string()]);
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
            "mcp__agentic-mcp__cli_ls",
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

        // Should disallow recursion/search tools but not cli_ls
        assert!(disallowed.contains(&"mcp__agentic-mcp__ask_agent".to_string()));
        assert!(disallowed.contains(&"mcp__agentic-mcp__cli_grep".to_string()));
        assert!(disallowed.contains(&"mcp__agentic-mcp__cli_glob".to_string()));
        assert!(!disallowed.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
    }

    #[test]
    fn test_disallowed_mcp_tools_locator_thoughts_allows_ls() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let disallowed = disallowed_mcp_tools_for(&enabled, AgentLocation::Thoughts);

        // Locator+Thoughts has cli_ls enabled, so only ask_agent/cli_* should be disallowed
        assert!(disallowed.contains(&"mcp__agentic-mcp__ask_agent".to_string()));
        assert!(disallowed.contains(&"mcp__agentic-mcp__cli_grep".to_string()));
        assert!(disallowed.contains(&"mcp__agentic-mcp__cli_glob".to_string()));
        assert!(!disallowed.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert_eq!(disallowed.len(), 3);
    }

    #[test]
    fn test_disallowed_mcp_tools_analyzer_web_allows_ls() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let disallowed = disallowed_mcp_tools_for(&enabled, AgentLocation::Web);

        // Analyzer+Web has cli_ls enabled, so only ask_agent/cli_* should be disallowed
        assert!(disallowed.contains(&"mcp__agentic-mcp__ask_agent".to_string()));
        assert!(disallowed.contains(&"mcp__agentic-mcp__cli_grep".to_string()));
        assert!(disallowed.contains(&"mcp__agentic-mcp__cli_glob".to_string()));
        assert!(!disallowed.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
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
    fn test_build_mcp_config_codebase() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let config = build_mcp_config(AgentLocation::Codebase, &enabled);
        assert!(config.mcp_servers.contains_key("agentic-mcp"));
        assert_eq!(config.mcp_servers.len(), 1); // Single server for all locations
    }

    #[test]
    fn test_build_mcp_config_thoughts() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let config = build_mcp_config(AgentLocation::Thoughts, &enabled);
        assert!(config.mcp_servers.contains_key("agentic-mcp"));
        assert_eq!(config.mcp_servers.len(), 1); // Single server for all locations
    }

    #[test]
    fn test_build_mcp_config_references() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::References);
        let config = build_mcp_config(AgentLocation::References, &enabled);
        assert!(config.mcp_servers.contains_key("agentic-mcp"));
        assert_eq!(config.mcp_servers.len(), 1); // Single server for all locations
    }

    #[test]
    fn test_build_mcp_config_web() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let config = build_mcp_config(AgentLocation::Web, &enabled);
        assert!(config.mcp_servers.contains_key("agentic-mcp"));
        assert_eq!(config.mcp_servers.len(), 1); // Single server for all locations
    }

    #[test]
    fn test_agentic_mcp_flags_locator_codebase() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let flags = agentic_mcp_flags(&enabled);
        // Locator+Codebase has mcp__agentic-mcp__cli_ls enabled
        assert!(flags.contains(&"--cli_ls".to_string()));
        assert!(!flags.contains(&"--ask_agent".to_string()));
        assert!(!flags.contains(&"--cli_grep".to_string()));
        assert!(!flags.contains(&"--cli_glob".to_string()));
    }

    #[test]
    fn test_agentic_mcp_flags_analyzer_codebase() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        let flags = agentic_mcp_flags(&enabled);
        // Analyzer+Codebase has mcp__agentic-mcp__cli_ls enabled
        assert!(flags.contains(&"--cli_ls".to_string()));
        assert!(!flags.contains(&"--ask_agent".to_string()));
    }

    #[test]
    fn test_agentic_mcp_flags_no_mcp_tools() {
        // When no agentic-mcp MCP tools are enabled, flags should be empty
        let enabled = vec!["Grep".to_string(), "Glob".to_string()];
        let flags = agentic_mcp_flags(&enabled);
        assert!(flags.is_empty());
    }

    #[test]
    fn test_agentic_mcp_flags_locator_thoughts() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let flags = agentic_mcp_flags(&enabled);
        // Locator+Thoughts has mcp__agentic-mcp__cli_ls enabled
        assert!(flags.contains(&"--cli_ls".to_string()));
        assert_eq!(flags.len(), 1);
    }

    #[test]
    fn test_agentic_mcp_flags_analyzer_web() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let flags = agentic_mcp_flags(&enabled);
        // Analyzer+Web has mcp__agentic-mcp__cli_ls enabled
        assert!(flags.contains(&"--cli_ls".to_string()));
        assert_eq!(flags.len(), 1);
    }

    #[test]
    fn test_agentic_mcp_flags_analyzer_thoughts() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Thoughts);
        let flags = agentic_mcp_flags(&enabled);
        // Analyzer+Thoughts has mcp__agentic-mcp__cli_ls enabled
        assert!(flags.contains(&"--cli_ls".to_string()));
        assert_eq!(flags.len(), 1);
    }

    #[test]
    fn test_agentic_mcp_flags_analyzer_references() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::References);
        let flags = agentic_mcp_flags(&enabled);
        // Analyzer+References has mcp__agentic-mcp__cli_ls enabled
        assert!(flags.contains(&"--cli_ls".to_string()));
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
