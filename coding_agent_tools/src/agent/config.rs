//! Mapping utilities for agent configuration.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use claudecode::config::{MCPConfig, MCPServer};
use claudecode::types::Model;
use universal_tool_core::prelude::ToolError;

use super::prompts::{
    CODEBASE_OVERLAY, REFERENCES_OVERLAY, THOUGHTS_OVERLAY, WEB_OVERLAY, compose_prompt_impl,
};
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

/// Get the allowed tools for a given type × location combination.
pub fn allowed_tools_for(agent_type: AgentType, location: AgentLocation) -> Vec<String> {
    use AgentLocation::*;
    use AgentType::*;

    match (agent_type, location) {
        (Locator, Codebase) => vec![
            "mcp__coding-agent-tools__ls".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, Thoughts) => vec![
            "mcp__thoughts__list_active_documents".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, References) => vec![
            "mcp__thoughts__list_references".into(),
            "Grep".into(),
            "Glob".into(),
        ],
        (Locator, Web) => vec!["WebSearch".into()],
        (Analyzer, Codebase) => vec![
            "Read".into(),
            "mcp__coding-agent-tools__ls".into(),
            "Grep".into(),
            "Glob".into(),
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
        ],
        (Analyzer, Web) => vec!["WebSearch".into(), "WebFetch".into()],
    }
}

/// Compose the system prompt for a given type × location combination.
pub fn compose_prompt(agent_type: AgentType, location: AgentLocation) -> String {
    let is_analyzer = matches!(agent_type, AgentType::Analyzer);
    let overlay = match location {
        AgentLocation::Codebase => CODEBASE_OVERLAY,
        AgentLocation::Thoughts => THOUGHTS_OVERLAY,
        AgentLocation::References => REFERENCES_OVERLAY,
        AgentLocation::Web => WEB_OVERLAY,
    };
    compose_prompt_impl(is_analyzer, overlay)
}

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

/// Check if a binary exists in PATH.
fn bin_in_path(cmd: &str) -> bool {
    if let Some(paths) = env::var_os("PATH") {
        for p in env::split_paths(&paths) {
            let candidate = p.join(cmd);
            if candidate.is_file() {
                return true;
            }
            #[cfg(windows)]
            {
                for ext in ["exe", "cmd", "bat"] {
                    let with_ext = candidate.with_extension(ext);
                    if with_ext.is_file() {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Verify required binaries exist for a given location.
/// - coding-agent-tools is always required
/// - thoughts_tool is required for thoughts/references locations
pub fn require_binaries_for_location(location: AgentLocation) -> Result<(), ToolError> {
    if !bin_in_path("coding-agent-tools") {
        return Err(ToolError::internal(
            "Required binary 'coding-agent-tools' not found in PATH. Please build and install it, or add it to PATH.",
        ));
    }
    if matches!(
        location,
        AgentLocation::Thoughts | AgentLocation::References
    ) && !bin_in_path("thoughts_tool")
    {
        return Err(ToolError::internal(
            "Required binary 'thoughts_tool' not found in PATH for thoughts/references location. Please install it or add it to PATH.",
        ));
    }
    Ok(())
}

/// Build MCP server configuration for a given location.
pub fn build_mcp_config(location: AgentLocation) -> MCPConfig {
    let mut servers: HashMap<String, MCPServer> = HashMap::new();

    // Always include coding-agent-tools server
    servers.insert(
        "coding-agent-tools".to_string(),
        MCPServer::stdio("coding-agent-tools", vec!["mcp".to_string()]),
    );

    // Include thoughts server when needed
    if matches!(
        location,
        AgentLocation::Thoughts | AgentLocation::References
    ) {
        servers.insert(
            "thoughts".to_string(),
            MCPServer::stdio("thoughts_tool", vec!["mcp".to_string()]),
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
    fn test_allowed_tools_locator_codebase() {
        let tools = allowed_tools_for(AgentType::Locator, AgentLocation::Codebase);
        assert!(tools.contains(&"mcp__coding-agent-tools__ls".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
        assert!(!tools.contains(&"Read".to_string())); // Locator doesn't read deeply
    }

    #[test]
    fn test_allowed_tools_analyzer_codebase() {
        let tools = allowed_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        assert!(tools.contains(&"Read".to_string())); // Analyzer can read
        assert!(tools.contains(&"mcp__coding-agent-tools__ls".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
    }

    #[test]
    fn test_allowed_tools_locator_thoughts() {
        let tools = allowed_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        assert!(tools.contains(&"mcp__thoughts__list_active_documents".to_string()));
        assert!(!tools.contains(&"mcp__coding-agent-tools__ls".to_string()));
    }

    #[test]
    fn test_allowed_tools_locator_references() {
        let tools = allowed_tools_for(AgentType::Locator, AgentLocation::References);
        assert!(tools.contains(&"mcp__thoughts__list_references".to_string()));
    }

    #[test]
    fn test_allowed_tools_locator_web() {
        let tools = allowed_tools_for(AgentType::Locator, AgentLocation::Web);
        assert_eq!(tools, vec!["WebSearch".to_string()]);
    }

    #[test]
    fn test_allowed_tools_analyzer_web() {
        let tools = allowed_tools_for(AgentType::Analyzer, AgentLocation::Web);
        assert!(tools.contains(&"WebSearch".to_string()));
        assert!(tools.contains(&"WebFetch".to_string()));
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
        let config = build_mcp_config(AgentLocation::Codebase);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(!config.mcp_servers.contains_key("thoughts"));
    }

    #[test]
    fn test_build_mcp_config_thoughts() {
        let config = build_mcp_config(AgentLocation::Thoughts);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(config.mcp_servers.contains_key("thoughts"));
    }

    #[test]
    fn test_build_mcp_config_references() {
        let config = build_mcp_config(AgentLocation::References);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(config.mcp_servers.contains_key("thoughts"));
    }

    #[test]
    fn test_build_mcp_config_web() {
        let config = build_mcp_config(AgentLocation::Web);
        assert!(config.mcp_servers.contains_key("coding-agent-tools"));
        assert!(!config.mcp_servers.contains_key("thoughts"));
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
                let tools = allowed_tools_for(agent_type, location);
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
