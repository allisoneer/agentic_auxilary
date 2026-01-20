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

/// Compose the system prompt for a given type × location combination.
pub fn compose_prompt(agent_type: AgentType, location: AgentLocation) -> String {
    compose_prompt_impl(agent_type, location)
}

/// Extract base tool names for our agentic-mcp server from enabled tool IDs.
/// Example: "mcp__agentic-mcp__cli_ls" -> "cli_ls".
/// Uses BTreeSet for deterministic ordering.
fn agentic_mcp_allowlist_from(enabled: &[String]) -> Vec<String> {
    use std::collections::BTreeSet;
    const PREFIX: &str = "mcp__agentic-mcp__";

    let mut set = BTreeSet::new();
    for e in enabled {
        if let Some(rest) = e.strip_prefix(PREFIX) {
            let name = rest.trim();
            if !name.is_empty() {
                set.insert(name.to_string());
            }
        }
    }
    set.into_iter().collect()
}

// NOTE: Binary existence checks (bin_in_path, require_binaries_for_location) have been removed.
// MCP server validation now happens via claudecode::mcp::validate in spawn_agent, which provides
// better error messages with stderr capture and actual handshake verification.

/// Build MCP server configuration for a given location, with tool allowlist.
/// Uses single agentic-mcp server with --allow flag for all locations.
/// Returns empty config if no MCP tools are enabled.
pub fn build_mcp_config(_location: AgentLocation, enabled_tools: &[String]) -> MCPConfig {
    let mut servers: HashMap<String, MCPServer> = HashMap::new();

    // Build allowlist from enabled MCP tools
    let allowlist = agentic_mcp_allowlist_from(enabled_tools);

    // If no MCP tools are enabled, do not expose the server at all
    if allowlist.is_empty() {
        return MCPConfig {
            mcp_servers: servers,
        };
    }

    // Use --allow "tool1,tool2" (no "mcp" subcommand, no individual flags)
    let args = vec!["--allow".to_string(), allowlist.join(",")];

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

    #[test]
    fn test_agentic_mcp_allowlist_locator_codebase() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["cli_ls"]);
    }

    #[test]
    fn test_agentic_mcp_allowlist_locator_web_empty_and_no_server() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Web);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert!(list.is_empty());

        let cfg = build_mcp_config(AgentLocation::Web, &enabled);
        assert!(!cfg.mcp_servers.contains_key("agentic-mcp"));
        assert_eq!(cfg.mcp_servers.len(), 0);
    }

    #[test]
    fn test_agentic_mcp_allowlist_locator_thoughts() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["cli_ls", "thoughts_list_documents"]);
    }

    #[test]
    fn test_agentic_mcp_allowlist_locator_references() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::References);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["cli_ls", "thoughts_list_references"]);
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_codebase() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["cli_ls"]);
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_thoughts() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Thoughts);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["cli_ls", "thoughts_list_documents"]);
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_references() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::References);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["cli_ls", "thoughts_list_references"]);
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_web() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let list = agentic_mcp_allowlist_from(&enabled);
        // Analyzer+Web includes cli_ls
        assert_eq!(list, vec!["cli_ls"]);
    }
}
