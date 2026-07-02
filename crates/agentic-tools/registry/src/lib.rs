//! Unified tool registry aggregating all agentic-tools domain registries.
//!
//! This crate provides a single entry point for building a `ToolRegistry` containing
//! all available tools from the various domain crates (`coding_agent_tools`, `pr_comments`,
//! `linear_tools`, `gpt5_reasoner`, `thoughts_mcp_tools`, `web_retrieval`).
//!
//! # Example
//!
//! ```ignore
//! use agentic_tools_registry::{AgenticTools, AgenticToolsConfig};
//!
//! // Build registry with all tools
//! let registry = AgenticTools::new(AgenticToolsConfig::default());
//! assert!(registry.len() >= 19);
//!
//! // Build registry with allowlist
//! let config = AgenticToolsConfig {
//!     allowlist: Some(["cli_ls", "cli_grep"].into_iter().map(String::from).collect()),
//!     ..Default::default()
//! };
//! let filtered = AgenticTools::new(config);
//! assert_eq!(filtered.len(), 2);
//! ```

#[cfg(not(unix))]
compile_error!(
    "agentic-tools-registry only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

use agentic_config::types::AnthropicServiceConfig;
use agentic_config::types::CliToolsConfig;
use agentic_config::types::DiscordServiceConfig;
use agentic_config::types::ExaServiceConfig;
use agentic_config::types::GitHubServiceConfig;
use agentic_config::types::LinearServiceConfig;
use agentic_config::types::ReasoningConfig;
use agentic_config::types::ReviewConfig;
use agentic_config::types::SubagentsConfig;
use agentic_config::types::ThoughtsConfig;
use agentic_config::types::WebRetrievalConfig;
use agentic_config::types::WorkspaceToolsConfig;
use agentic_tools_core::ToolRegistry;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;

/// Configuration for building the unified registry.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgenticToolsConfig {
    /// Optional allowlist of tool names (case-insensitive).
    /// Empty or None = enable all tools.
    #[serde(default)]
    pub allowlist: Option<HashSet<String>>,

    /// Tool-specific config for coding-agent-tools subagents.
    #[serde(default)]
    pub subagents: SubagentsConfig,

    /// Tool-specific config for CLI tools (limits, ignore patterns).
    #[serde(default)]
    pub cli_tools: CliToolsConfig,

    /// Tool-specific config for workspace-local file and todo tools.
    #[serde(default)]
    pub workspace_tools: WorkspaceToolsConfig,

    /// Tool-specific config for gpt5-reasoner.
    #[serde(default)]
    pub reasoning: ReasoningConfig,

    /// Tool-specific config for web retrieval tools.
    #[serde(default)]
    pub web_retrieval: WebRetrievalConfig,

    /// Anthropic service configuration for web summarization.
    #[serde(default)]
    pub anthropic: AnthropicServiceConfig,

    /// Exa service configuration for web search.
    #[serde(default)]
    pub exa: ExaServiceConfig,

    /// Linear service configuration for linear tools.
    #[serde(default)]
    pub linear: LinearServiceConfig,

    /// GitHub service configuration for `pr_comments`.
    #[serde(default)]
    pub github: GitHubServiceConfig,

    /// Discord service configuration for discord tools.
    #[serde(default)]
    pub discord: DiscordServiceConfig,

    /// Review tools configuration.
    #[serde(default)]
    pub review: ReviewConfig,

    /// Thoughts tools configuration.
    #[serde(default)]
    pub thoughts: ThoughtsConfig,

    /// Reserved for future use (e.g., schema strictness, patches).
    #[serde(default)]
    pub extras: serde_json::Value,
}

/// Unified `AgenticTools` entrypoint.
pub struct AgenticTools;

// Tool name constants for each domain
const CODING_NAMES: &[&str] = &[
    "cli_ls",
    "ask_agent",
    "cli_grep",
    "cli_glob",
    "cli_just_search",
    "cli_just_execute",
];

const PR_COMMENTS_NAMES: &[&str] = &["gh_get_comments", "gh_add_comment_reply", "gh_get_prs"];

const LINEAR_NAMES: &[&str] = &[
    "linear_search_issues",
    "linear_read_issue",
    "linear_create_issue",
    "linear_add_comment",
    "linear_get_issue_comments",
    "linear_archive_issue",
    "linear_update_issue",
    "linear_set_relation",
    "linear_get_metadata",
];

const DISCORD_NAMES: &[&str] = &["discord_search_messages"];

const GPT5_NAMES: &[&str] = &["ask_reasoning_model"];

const THOUGHTS_NAMES: &[&str] = &[
    "thoughts_write_document",
    "thoughts_list_documents",
    "thoughts_list_references",
    "thoughts_get_repo_refs",
    "thoughts_add_reference",
    "thoughts_get_template",
];

const WEB_NAMES: &[&str] = &["web_fetch", "web_search"];

const REVIEW_NAMES: &[&str] = &["review_diff_snapshot", "review_diff_page", "review_run"];

const WORKSPACE_NAMES: &[&str] = &[
    "workspace_read",
    "workspace_todowrite",
    "workspace_edit",
    "workspace_apply_patch",
];

impl AgenticTools {
    /// Build the unified `ToolRegistry` using domain registries.
    ///
    /// Lazy domain gating: When an allowlist is provided, only build domains
    /// whose tools intersect the allowlist.
    #[expect(
        clippy::allow_attributes,
        reason = "incremental legacy lint mitigation for pre-existing API shape"
    )]
    // TODO(3): clean up new_ret_no_self as part of broader agentic-tools-registry lint conformance pass.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(config: AgenticToolsConfig) -> ToolRegistry {
        let allow = normalize_allowlist(config.allowlist);

        // Helper: decide if a domain should be built
        let domain_wanted = |names: &[&str]| match &allow {
            None => true,
            Some(set) => names.iter().any(|n| set.contains(&n.to_lowercase())),
        };

        // Accumulate selected domain registries
        let mut regs = Vec::new();

        // coding_agent_tools (6 tools)
        if domain_wanted(CODING_NAMES) {
            regs.push(coding_agent_tools::build_registry(
                config.subagents.clone(),
                config.cli_tools.clone(),
            ));
        }

        // pr_comments (3 tools)
        if domain_wanted(PR_COMMENTS_NAMES) {
            // TODO(2): Centralize ambient git repo detection + overrides across tool registries
            // (avoid per-domain fallbacks like this).
            let tool = match pr_comments::PrComments::with_config(config.github.clone()) {
                Ok(t) => t,
                Err(e) => {
                    warn!(
                        "pr_comments: ambient repo detection failed ({}); tools will return a clear error until repo context is available",
                        e
                    );
                    pr_comments::PrComments::disabled_with_config(
                        format!("{e:#}"),
                        config.github.clone(),
                    )
                }
            };
            regs.push(pr_comments::build_registry(Arc::new(tool)));
        }

        // linear_tools (9 tools)
        if domain_wanted(LINEAR_NAMES) {
            let linear = Arc::new(linear_tools::LinearTools::with_config(
                config.linear.clone(),
            ));
            regs.push(linear_tools::build_registry(linear));
        }

        // discord-tools (1 tool)
        if domain_wanted(DISCORD_NAMES) {
            let discord = Arc::new(discord_tools::DiscordTools::with_config(
                config.discord.clone(),
            ));
            regs.push(discord_tools::build_registry(discord));
        }

        // gpt5_reasoner (1 tool)
        if domain_wanted(GPT5_NAMES) {
            regs.push(gpt5_reasoner::build_registry(config.reasoning.clone()));
        }

        // thoughts-mcp-tools (6 tools)
        if domain_wanted(THOUGHTS_NAMES) {
            regs.push(thoughts_mcp_tools::build_registry(config.thoughts.clone()));
        }

        // web-retrieval (2 tools)
        if domain_wanted(WEB_NAMES) {
            let web = Arc::new(web_retrieval::WebTools::with_config(
                config.web_retrieval.clone(),
                &config.exa,
                config.anthropic.clone(),
            ));
            regs.push(web_retrieval::build_registry(web));
        }

        // review_tools (3 tools)
        if domain_wanted(REVIEW_NAMES) {
            let svc = Arc::new(review_tools::ReviewTools::with_config(config.review));
            regs.push(review_tools::build_registry(svc));
        }

        if workspace_tools_enabled(&config.workspace_tools) && domain_wanted(WORKSPACE_NAMES) {
            regs.push(workspace_tools::build_registry(&config.workspace_tools));
        }

        let merged = ToolRegistry::merge_all(regs);

        // Final allowlist filtering at registry level (authoritative)
        if let Some(set) = allow {
            let names: Vec<&str> = set.iter().map(String::as_str).collect();
            // Warn about unknown tool names in allowlist
            for name in &names {
                if !merged.contains(name) {
                    warn!("Unknown tool in allowlist: {}", name);
                }
            }
            merged.subset(names)
        } else {
            merged
        }
    }

    /// Get the total count of available tools when no allowlist is applied.
    pub fn total_tool_count() -> usize {
        CODING_NAMES.len()
            + PR_COMMENTS_NAMES.len()
            + LINEAR_NAMES.len()
            + DISCORD_NAMES.len()
            + GPT5_NAMES.len()
            + THOUGHTS_NAMES.len()
            + WEB_NAMES.len()
            + REVIEW_NAMES.len()
            + WORKSPACE_NAMES.len()
    }
}

fn workspace_tools_enabled(config: &WorkspaceToolsConfig) -> bool {
    config.workspace_read
        || config.workspace_todowrite
        || config.workspace_edit
        || config.workspace_apply_patch
}

/// Normalize allowlist: lowercase, trim, filter empty strings.
/// Returns None if the resulting set is empty (empty allowlist = all tools).
fn normalize_allowlist(allowlist: Option<HashSet<String>>) -> Option<HashSet<String>> {
    allowlist.and_then(|s| {
        let normalized: HashSet<String> = s
            .into_iter()
            .map(|n| n.trim().to_lowercase())
            .filter(|n| !n.is_empty())
            .collect();
        if normalized.is_empty() {
            None // Empty allowlist = enable all tools
        } else {
            Some(normalized)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_tool_count_is_35() {
        assert_eq!(AgenticTools::total_tool_count(), 35);
    }

    #[test]
    fn normalize_allowlist_lowercases() {
        let mut set = HashSet::new();
        set.insert("CLI_LS".to_string());
        set.insert("Ask_Reasoning_Model".to_string());
        let normalized = normalize_allowlist(Some(set)).unwrap();
        assert!(normalized.contains("cli_ls"));
        assert!(normalized.contains("ask_reasoning_model"));
        assert!(!normalized.contains("CLI_LS"));
    }

    #[test]
    fn normalize_allowlist_filters_empty() {
        let mut set = HashSet::new();
        set.insert(String::new());
        set.insert("   ".to_string());
        set.insert("cli_ls".to_string());
        let normalized = normalize_allowlist(Some(set)).unwrap();
        assert_eq!(normalized.len(), 1);
        assert!(normalized.contains("cli_ls"));
    }

    #[test]
    fn normalize_allowlist_none_returns_none() {
        assert!(normalize_allowlist(None).is_none());
    }

    // Integration tests for AgenticTools::new
    // Note: These tests actually build the full registries, which may have
    // side effects (e.g., pr_comments tries git detection, linear reads env var).
    // The fallbacks ensure they don't fail in test environments.

    #[test]
    fn allowlist_none_builds_all_tools() {
        let reg = AgenticTools::new(AgenticToolsConfig::default());
        let names = reg.list_names();

        // Workspace tools remain disabled by default.
        assert!(
            names.len() >= 27,
            "expected at least 27 tools, got {}",
            names.len()
        );

        // Check some known tools from each domain
        assert!(
            reg.contains("cli_ls"),
            "missing cli_ls from coding_agent_tools"
        );
        assert!(
            reg.contains("gh_get_comments"),
            "missing gh_get_comments from pr_comments"
        );
        assert!(
            reg.contains("linear_search_issues"),
            "missing linear_search_issues from linear_tools"
        );
        assert!(
            reg.contains("discord_search_messages"),
            "missing discord_search_messages from discord-tools"
        );
        assert!(
            reg.contains("ask_reasoning_model"),
            "missing ask_reasoning_model from gpt5_reasoner"
        );
        assert!(
            reg.contains("thoughts_add_reference"),
            "missing thoughts_add_reference from thoughts_mcp_tools"
        );
        assert!(
            reg.contains("thoughts_get_repo_refs"),
            "missing thoughts_get_repo_refs from thoughts_mcp_tools"
        );
        assert!(
            reg.contains("web_fetch"),
            "missing web_fetch from web_retrieval"
        );
        assert!(
            reg.contains("web_search"),
            "missing web_search from web_retrieval"
        );
        assert!(!reg.contains("workspace_read"));
        assert!(!reg.contains("workspace_todowrite"));
        assert!(!reg.contains("workspace_edit"));
        assert!(!reg.contains("workspace_apply_patch"));
    }

    #[test]
    fn allowlist_filters_to_specific_tools() {
        let mut set = HashSet::new();
        set.insert("cli_ls".to_string());
        set.insert("ask_reasoning_model".to_string());
        let config = AgenticToolsConfig {
            allowlist: Some(set),
            ..Default::default()
        };

        let reg = AgenticTools::new(config);
        let names = reg.list_names();

        assert_eq!(names.len(), 2);
        assert!(reg.contains("cli_ls"));
        assert!(reg.contains("ask_reasoning_model"));
        assert!(!reg.contains("cli_grep"));
    }

    #[test]
    fn allowlist_is_case_insensitive() {
        let mut set = HashSet::new();
        set.insert("CLI_LS".to_string());
        set.insert("ASK_REASONING_MODEL".to_string());
        let config = AgenticToolsConfig {
            allowlist: Some(set),
            ..Default::default()
        };

        let reg = AgenticTools::new(config);

        // Should find tools despite uppercase allowlist
        assert!(reg.contains("cli_ls"));
        assert!(reg.contains("ask_reasoning_model"));
    }

    #[test]
    fn empty_allowlist_enables_all_tools() {
        let config = AgenticToolsConfig {
            allowlist: Some(HashSet::new()),
            ..Default::default()
        };

        let reg = AgenticTools::new(config);

        // Empty allowlist normalizes to None, enabling all tools
        assert!(reg.len() >= 27);
    }

    #[test]
    fn allowlist_web_search_only() {
        let mut set = HashSet::new();
        set.insert("web_search".to_string());
        let config = AgenticToolsConfig {
            allowlist: Some(set),
            ..Default::default()
        };

        let reg = AgenticTools::new(config);
        assert_eq!(reg.len(), 1);
        assert!(reg.contains("web_search"));
        assert!(!reg.contains("web_fetch"));
    }

    #[test]
    fn allowlist_single_linear_update_issue() {
        let mut set = HashSet::new();
        set.insert("linear_update_issue".to_string());
        let config = AgenticToolsConfig {
            allowlist: Some(set),
            ..Default::default()
        };

        let reg = AgenticTools::new(config);

        assert_eq!(reg.len(), 1, "expected exactly 1 tool");
        assert!(
            reg.contains("linear_update_issue"),
            "linear_update_issue must be present after single-tool allowlist"
        );
        assert!(!reg.contains("linear_search_issues"));
        assert!(!reg.contains("linear_read_issue"));
    }

    #[test]
    fn allowlist_discord_search_only() {
        let mut set = HashSet::new();
        set.insert("discord_search_messages".to_string());
        let reg = AgenticTools::new(AgenticToolsConfig {
            allowlist: Some(set),
            ..Default::default()
        });

        assert_eq!(reg.len(), 1);
        assert!(reg.contains("discord_search_messages"));
    }

    #[test]
    fn unknown_allowlist_names_are_ignored() {
        let mut set = HashSet::new();
        set.insert("cli_ls".to_string());
        set.insert("nonexistent_tool".to_string());
        let config = AgenticToolsConfig {
            allowlist: Some(set),
            ..Default::default()
        };

        let reg = AgenticTools::new(config);

        // Should only have "cli_ls", ignoring "nonexistent_tool"
        assert_eq!(reg.len(), 1);
        assert!(reg.contains("cli_ls"));
    }

    #[test]
    fn builds_with_non_default_tool_configs() {
        // Verify that AgenticTools::new() builds successfully with non-default
        // SubagentsConfig and ReasoningConfig values
        let config = AgenticToolsConfig {
            allowlist: None,
            subagents: SubagentsConfig {
                locator_model: "custom-haiku".into(),
                analyzer_model: "custom-sonnet".into(),
                runtime_timeout_secs: 3600,
            },
            reasoning: ReasoningConfig {
                optimizer_model: "anthropic/custom-optimizer".into(),
                executor_model: "openai/custom-executor".into(),
                reasoning_effort: Some("high".into()),
                api_base_url: None,
                max_input_tokens: None,
                max_completion_tokens: Some(128_000),
                executor_timeout_secs: 2700,
                empty_response_no_retry_after_secs: 600,
                stream_heartbeat_secs: 30,
            },
            ..Default::default()
        };

        let reg = AgenticTools::new(config);

        // Should build successfully with all tools
        assert!(
            reg.len() >= 23,
            "expected at least 23 tools, got {}",
            reg.len()
        );

        // Verify tools from domains that use the configs are present
        assert!(
            reg.contains("ask_agent"),
            "missing ask_agent (uses subagents config)"
        );
        assert!(
            reg.contains("ask_reasoning_model"),
            "missing ask_reasoning_model (uses reasoning config)"
        );
    }

    #[test]
    fn workspace_tools_require_matching_toggle() {
        let reg = AgenticTools::new(AgenticToolsConfig {
            workspace_tools: WorkspaceToolsConfig {
                workspace_read: true,
                ..Default::default()
            },
            ..Default::default()
        });

        assert!(reg.contains("workspace_read"));
        assert!(!reg.contains("workspace_todowrite"));
        assert!(!reg.contains("workspace_edit"));
        assert!(!reg.contains("workspace_apply_patch"));
    }

    #[test]
    fn workspace_tools_respect_allowlist_intersection() {
        let reg = AgenticTools::new(AgenticToolsConfig {
            allowlist: Some(HashSet::from([
                String::from("workspace_read"),
                String::from("workspace_edit"),
            ])),
            workspace_tools: WorkspaceToolsConfig {
                workspace_read: true,
                workspace_edit: true,
                workspace_apply_patch: true,
                ..Default::default()
            },
            ..Default::default()
        });

        assert!(reg.contains("workspace_read"));
        assert!(reg.contains("workspace_edit"));
        assert!(!reg.contains("workspace_todowrite"));
        assert!(!reg.contains("workspace_apply_patch"));
    }
}
