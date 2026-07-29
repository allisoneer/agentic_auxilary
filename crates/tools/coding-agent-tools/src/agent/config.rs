//! Mapping utilities for agent configuration.

use std::collections::HashMap;

use agentic_config::types::SubagentsConfig;
use claudecode::config::MCPConfig;
use claudecode::config::MCPServer;
use claudecode::types::Model;

use super::prompts::compose_prompt_impl;
use crate::types::AgentLocation;
use crate::types::AgentType;

/// Select model for agent type based on config.
///
/// Maps config strings to claudecode Model enum variants.
/// TODO(2): claudecode SDK could be enhanced with Custom(String) variant
/// for more flexibility, but enum mapping works for now.
pub fn model_for(agent_type: AgentType, cfg: &SubagentsConfig) -> Result<Model, String> {
    let (field, raw) = match agent_type {
        AgentType::Locator => ("subagents.locator_model", cfg.locator_model.as_str()),
        AgentType::Analyzer => ("subagents.analyzer_model", cfg.analyzer_model.as_str()),
    };

    // Map known model strings to enum variants
    match raw.trim().to_lowercase().as_str() {
        "haiku" | "claude-haiku-4-5" => Ok(Model::Haiku),
        "sonnet" | "claude-sonnet-4-6" => Ok(Model::Sonnet),
        "opus" | "claude-opus-4-6" => Ok(Model::Opus),
        _ => Err(format!(
            "Invalid {field} value `{raw}`; expected haiku, claude-haiku-4-5, sonnet, claude-sonnet-4-6, opus, or claude-opus-4-6"
        )),
    }
}

// TODO(2): Intentional explicit match for clarity and compile-time exhaustiveness.
// We keep the hardcoded mapping to avoid accidental tool exposure and ensure deterministic tests.
/// Get the enabled tools for a given type × location combination.
/// Every entry is an eagerly published qualified MCP tool; Claude built-ins are forbidden.
pub fn enabled_tools_for(agent_type: AgentType, location: AgentLocation) -> Vec<String> {
    use AgentLocation::Codebase;
    use AgentLocation::References;
    use AgentLocation::Thoughts;
    use AgentLocation::Web;
    use AgentType::Analyzer;
    use AgentType::Locator;

    match (agent_type, location) {
        (Locator, Codebase) => vec![
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__cli_grep".into(),
            "mcp__agentic-mcp__cli_glob".into(),
        ],
        (Locator, Thoughts) => vec![
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_documents".into(),
            "mcp__agentic-mcp__cli_grep".into(),
            "mcp__agentic-mcp__cli_glob".into(),
        ],
        (Locator, References) => vec![
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_references".into(),
            "mcp__agentic-mcp__cli_grep".into(),
            "mcp__agentic-mcp__cli_glob".into(),
        ],
        (Locator, Web) => vec![
            "mcp__agentic-mcp__web_search".into(),
            "mcp__agentic-mcp__web_fetch".into(),
        ],
        (Analyzer, Codebase) => vec![
            "mcp__agentic-mcp__workspace_read".into(),
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__cli_grep".into(),
            "mcp__agentic-mcp__cli_glob".into(),
            "mcp__agentic-mcp__workspace_todowrite".into(),
        ],
        (Analyzer, Thoughts) => vec![
            "mcp__agentic-mcp__thoughts_read_document".into(),
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_documents".into(),
            "mcp__agentic-mcp__cli_grep".into(),
            "mcp__agentic-mcp__cli_glob".into(),
        ],
        (Analyzer, References) => vec![
            "mcp__agentic-mcp__thoughts_read_reference".into(),
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__thoughts_list_references".into(),
            "mcp__agentic-mcp__cli_grep".into(),
            "mcp__agentic-mcp__cli_glob".into(),
            "mcp__agentic-mcp__workspace_todowrite".into(),
        ],
        (Analyzer, Web) => vec![
            "mcp__agentic-mcp__web_search".into(),
            "mcp__agentic-mcp__web_fetch".into(),
            "mcp__agentic-mcp__workspace_todowrite".into(),
        ],
    }
}

/// Compose the system prompt for a given type × location combination.
pub fn compose_prompt(agent_type: AgentType, location: AgentLocation) -> String {
    compose_prompt_impl(agent_type, location)
}

pub(crate) fn compose_prompt_with_guidance(
    agent_type: AgentType,
    location: AgentLocation,
    guidance: &str,
) -> String {
    let mut prompt = compose_prompt(agent_type, location);
    if !guidance.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(guidance);
    }
    prompt
}

/// Extract base tool names for our agentic-mcp server from enabled tool IDs.
/// Example: "mcp__agentic-mcp__cli_ls" -> "`cli_ls`".
/// Uses `BTreeSet` for deterministic ordering.
pub(crate) fn agentic_mcp_allowlist_from(enabled: &[String]) -> Vec<String> {
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
// MCP server validation now happens via claudecode::mcp::validate in ask_agent, which provides
// better error messages with stderr capture and actual handshake verification.

/// Build MCP server configuration for a given location, with tool allowlist.
/// Uses single agentic-mcp server with --allow flag for all locations.
/// Returns empty config if no MCP tools are enabled.
pub fn build_mcp_config(location: AgentLocation, enabled_tools: &[String]) -> MCPConfig {
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
    let args = vec![
        "--nested-profile".to_string(),
        match location {
            AgentLocation::Codebase => "codebase",
            AgentLocation::Thoughts => "thoughts",
            AgentLocation::References => "references",
            AgentLocation::Web => "web",
        }
        .to_string(),
        "--allow".to_string(),
        allowlist.join(","),
        "--suppress-search-reminder".to_string(),
    ];

    servers.insert(
        "agentic-mcp".to_string(),
        MCPServer::stdio("agentic-mcp", args),
    );

    MCPConfig {
        mcp_servers: servers,
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests should fail immediately on fixture and assertion errors"
)]
mod tests {
    use super::*;

    #[test]
    fn test_model_for_locator_default() {
        let cfg = SubagentsConfig::default();
        assert_eq!(model_for(AgentType::Locator, &cfg).unwrap(), Model::Haiku);
    }

    #[test]
    fn test_model_for_analyzer_default() {
        let cfg = SubagentsConfig::default();
        assert_eq!(model_for(AgentType::Analyzer, &cfg).unwrap(), Model::Sonnet);
    }

    #[test]
    fn test_model_for_with_custom_config() {
        let cfg = SubagentsConfig {
            locator_model: "sonnet".into(),
            analyzer_model: "opus".into(),
            runtime_timeout_secs: 3600,
        };
        assert_eq!(model_for(AgentType::Locator, &cfg).unwrap(), Model::Sonnet);
        assert_eq!(model_for(AgentType::Analyzer, &cfg).unwrap(), Model::Opus);
    }

    #[test]
    fn test_model_for_rejects_unknown_values() {
        let cfg = SubagentsConfig {
            locator_model: "unknown-model".into(),
            analyzer_model: "another-unknown".into(),
            runtime_timeout_secs: 3600,
        };
        let locator = model_for(AgentType::Locator, &cfg).unwrap_err();
        let analyzer = model_for(AgentType::Analyzer, &cfg).unwrap_err();
        assert!(locator.contains("subagents.locator_model"));
        assert!(locator.contains("unknown-model"));
        assert!(analyzer.contains("subagents.analyzer_model"));

        let empty = SubagentsConfig {
            locator_model: String::new(),
            analyzer_model: String::new(),
            runtime_timeout_secs: 3600,
        };
        let locator = model_for(AgentType::Locator, &empty).unwrap_err();
        let analyzer = model_for(AgentType::Analyzer, &empty).unwrap_err();
        assert!(locator.contains("subagents.locator_model"));
        assert!(locator.contains("``"));
        assert!(analyzer.contains("subagents.analyzer_model"));
        assert!(analyzer.contains("``"));
    }

    #[test]
    fn locator_default_model_string_is_explicitly_recognized() {
        let mut cfg = SubagentsConfig::default();

        // Use the default locator model string for the Analyzer slot.
        // If this string stops being explicitly recognized by `model_for()`,
        // the Analyzer fallback would return Sonnet (wrong for this assertion).
        let locator_default = cfg.locator_model.clone();
        cfg.analyzer_model = locator_default;

        assert_eq!(model_for(AgentType::Analyzer, &cfg).unwrap(), Model::Haiku);
    }

    #[test]
    fn analyzer_default_model_string_is_explicitly_recognized() {
        let mut cfg = SubagentsConfig::default();

        // Use the default analyzer model string for the Locator slot.
        // If this string stops being explicitly recognized by `model_for()`,
        // the Locator fallback would return Haiku (wrong for this assertion).
        let analyzer_default = cfg.analyzer_model.clone();
        cfg.locator_model = analyzer_default;

        assert_eq!(model_for(AgentType::Locator, &cfg).unwrap(), Model::Sonnet);
    }

    #[test]
    fn test_enabled_tools_locator_codebase() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_grep".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_glob".to_string()));
        assert!(!tools.contains(&"Read".to_string())); // Locator doesn't read deeply
    }

    #[test]
    fn test_enabled_tools_analyzer_codebase() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        assert!(tools.contains(&"mcp__agentic-mcp__workspace_read".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_grep".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_glob".to_string()));
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
        assert!(tools.contains(&"mcp__agentic-mcp__thoughts_read_document".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_grep".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_glob".to_string()));
    }

    #[test]
    fn test_enabled_tools_analyzer_references_has_ls() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::References);
        assert!(tools.contains(&"mcp__agentic-mcp__cli_ls".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__thoughts_list_references".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__thoughts_read_reference".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_grep".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__cli_glob".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__workspace_todowrite".to_string()));
    }

    #[test]
    fn test_enabled_tools_locator_web() {
        let tools = enabled_tools_for(AgentType::Locator, AgentLocation::Web);
        assert_eq!(
            tools,
            vec![
                "mcp__agentic-mcp__web_search".to_string(),
                "mcp__agentic-mcp__web_fetch".to_string()
            ]
        );
    }

    #[test]
    fn test_enabled_tools_analyzer_web() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        assert!(tools.contains(&"mcp__agentic-mcp__web_search".to_string()));
        assert!(tools.contains(&"mcp__agentic-mcp__web_fetch".to_string()));
    }

    #[test]
    fn test_enabled_tools_analyzer_web_full_set() {
        let tools = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let expected = [
            "mcp__agentic-mcp__web_search",
            "mcp__agentic-mcp__web_fetch",
            "mcp__agentic-mcp__workspace_todowrite",
        ];
        for t in expected {
            assert!(tools.contains(&t.to_string()), "missing tool: {t}");
        }
        assert_eq!(tools.len(), 3);
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
        assert!(prompt.contains("web_fetch"));
    }

    #[test]
    fn composed_prompt_appends_tracked_guidance_exactly_once() {
        let guidance = "TRACKED-GUIDANCE-SENTINEL";
        let prompt =
            compose_prompt_with_guidance(AgentType::Locator, AgentLocation::Codebase, guidance);
        assert_eq!(prompt.matches(guidance).count(), 1);
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
                    "No tools for {agent_type:?} + {location:?}"
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
                    "Empty prompt for {agent_type:?} + {location:?}"
                );
                assert!(
                    prompt.len() > 100,
                    "Prompt too short for {agent_type:?} + {location:?}"
                );
            }
        }
    }

    #[test]
    fn test_agentic_mcp_allowlist_locator_codebase() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["cli_glob", "cli_grep", "cli_ls"]);
    }

    #[test]
    fn test_agentic_mcp_allowlist_locator_web_and_server_present() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Web);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["web_fetch", "web_search"]);

        let cfg = build_mcp_config(AgentLocation::Web, &enabled);
        assert!(cfg.mcp_servers.contains_key("agentic-mcp"));
        assert_eq!(cfg.mcp_servers.len(), 1);
    }

    #[test]
    fn test_agentic_mcp_allowlist_locator_thoughts() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Thoughts);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(
            list,
            vec!["cli_glob", "cli_grep", "cli_ls", "thoughts_list_documents"]
        );
    }

    #[test]
    fn test_agentic_mcp_allowlist_locator_references() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::References);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(
            list,
            vec!["cli_glob", "cli_grep", "cli_ls", "thoughts_list_references"]
        );
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_codebase() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Codebase);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(
            list,
            vec![
                "cli_glob",
                "cli_grep",
                "cli_ls",
                "workspace_read",
                "workspace_todowrite"
            ]
        );
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_thoughts() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Thoughts);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(
            list,
            vec![
                "cli_glob",
                "cli_grep",
                "cli_ls",
                "thoughts_list_documents",
                "thoughts_read_document"
            ]
        );
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_references() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::References);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(
            list,
            vec![
                "cli_glob",
                "cli_grep",
                "cli_ls",
                "thoughts_list_references",
                "thoughts_read_reference",
                "workspace_todowrite"
            ]
        );
    }

    #[test]
    fn test_agentic_mcp_allowlist_analyzer_web() {
        let enabled = enabled_tools_for(AgentType::Analyzer, AgentLocation::Web);
        let list = agentic_mcp_allowlist_from(&enabled);
        assert_eq!(list, vec!["web_fetch", "web_search", "workspace_todowrite"]);
    }

    #[test]
    fn test_build_mcp_config_includes_suppress_search_reminder_flag() {
        let enabled = enabled_tools_for(AgentType::Locator, AgentLocation::Codebase);
        let config = build_mcp_config(AgentLocation::Codebase, &enabled);
        let Some(server) = config.mcp_servers.get("agentic-mcp") else {
            panic!("expected agentic-mcp server to be configured");
        };

        match server {
            MCPServer::Stdio { command, args, .. } => {
                assert_eq!(command, "agentic-mcp");
                assert!(args.contains(&"--suppress-search-reminder".to_string()));
                assert!(
                    args.windows(2)
                        .any(|pair| pair == ["--nested-profile", "codebase"])
                );
            }
            MCPServer::Http { .. } => panic!("expected stdio MCP server"),
        }
    }

    #[test]
    fn every_cell_has_zero_builtins_and_exact_mcp_server() {
        for agent_type in [AgentType::Locator, AgentType::Analyzer] {
            for location in [
                AgentLocation::Codebase,
                AgentLocation::Thoughts,
                AgentLocation::References,
                AgentLocation::Web,
            ] {
                let tools = enabled_tools_for(agent_type, location);
                assert!(
                    tools
                        .iter()
                        .all(|tool| tool.starts_with("mcp__agentic-mcp__"))
                );
                let config = build_mcp_config(location, &tools);
                match &config.mcp_servers["agentic-mcp"] {
                    MCPServer::Stdio { command, .. } => assert_eq!(command, "agentic-mcp"),
                    MCPServer::Http { .. } => panic!("expected stdio server"),
                }
            }
        }
    }

    #[test]
    fn exact_eight_cell_matrix_is_locked() {
        let cases = [
            (
                AgentType::Locator,
                AgentLocation::Codebase,
                vec!["cli_ls", "cli_grep", "cli_glob"],
            ),
            (
                AgentType::Locator,
                AgentLocation::Thoughts,
                vec!["cli_ls", "thoughts_list_documents", "cli_grep", "cli_glob"],
            ),
            (
                AgentType::Locator,
                AgentLocation::References,
                vec!["cli_ls", "thoughts_list_references", "cli_grep", "cli_glob"],
            ),
            (
                AgentType::Locator,
                AgentLocation::Web,
                vec!["web_search", "web_fetch"],
            ),
            (
                AgentType::Analyzer,
                AgentLocation::Codebase,
                vec![
                    "workspace_read",
                    "cli_ls",
                    "cli_grep",
                    "cli_glob",
                    "workspace_todowrite",
                ],
            ),
            (
                AgentType::Analyzer,
                AgentLocation::Thoughts,
                vec![
                    "thoughts_read_document",
                    "cli_ls",
                    "thoughts_list_documents",
                    "cli_grep",
                    "cli_glob",
                ],
            ),
            (
                AgentType::Analyzer,
                AgentLocation::References,
                vec![
                    "thoughts_read_reference",
                    "cli_ls",
                    "thoughts_list_references",
                    "cli_grep",
                    "cli_glob",
                    "workspace_todowrite",
                ],
            ),
            (
                AgentType::Analyzer,
                AgentLocation::Web,
                vec!["web_search", "web_fetch", "workspace_todowrite"],
            ),
        ];
        for (agent_type, location, expected) in cases {
            assert_eq!(
                enabled_tools_for(agent_type, location),
                expected
                    .into_iter()
                    .map(|name| format!("mcp__agentic-mcp__{name}"))
                    .collect::<Vec<_>>()
            );
        }
    }
}
