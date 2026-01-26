//! Unified tool registry aggregating all agentic-tools domain registries.
//!
//! This crate provides a single entry point for building a `ToolRegistry` containing
//! all available tools from the various domain crates (coding_agent_tools, pr_comments,
//! linear_tools, gpt5_reasoner, thoughts_mcp_tools).
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

use agentic_tools_core::ToolRegistry;
use serde::{Deserialize, Serialize};
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

    /// Reserved for future use (e.g., schema strictness, patches).
    #[serde(default)]
    pub extras: serde_json::Value,
}

/// Unified AgenticTools entrypoint.
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
];

const GPT5_NAMES: &[&str] = &["ask_reasoning_model"];

const THOUGHTS_NAMES: &[&str] = &[
    "thoughts_write_document",
    "thoughts_list_documents",
    "thoughts_list_references",
    "thoughts_add_reference",
    "thoughts_get_template",
];

impl AgenticTools {
    /// Build the unified ToolRegistry using domain registries.
    ///
    /// Lazy domain gating: When an allowlist is provided, only build domains
    /// whose tools intersect the allowlist.
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
            let svc = Arc::new(coding_agent_tools::CodingAgentTools::new());
            regs.push(coding_agent_tools::build_registry(svc));
        }

        // pr_comments (3 tools)
        if domain_wanted(PR_COMMENTS_NAMES) {
            // TODO(2): Centralize ambient git repo detection + overrides across tool registries
            // (avoid per-domain fallbacks like this).
            let tool = match pr_comments::PrComments::new() {
                Ok(t) => t,
                Err(e) => {
                    warn!(
                        "pr_comments: ambient repo detection failed ({}); tools will return a clear error until repo context is available",
                        e
                    );
                    pr_comments::PrComments::disabled(format!("{:#}", e))
                }
            };
            regs.push(pr_comments::build_registry(Arc::new(tool)));
        }

        // linear_tools (4 tools)
        if domain_wanted(LINEAR_NAMES) {
            let linear = Arc::new(linear_tools::LinearTools::new());
            regs.push(linear_tools::build_registry(linear));
        }

        // gpt5_reasoner (1 tool)
        if domain_wanted(GPT5_NAMES) {
            regs.push(gpt5_reasoner::build_registry());
        }

        // thoughts-mcp-tools (5 tools)
        if domain_wanted(THOUGHTS_NAMES) {
            regs.push(thoughts_mcp_tools::build_registry());
        }

        let merged = ToolRegistry::merge_all(regs);

        // Final allowlist filtering at registry level (authoritative)
        if let Some(set) = allow {
            let names: Vec<&str> = set.iter().map(|s| s.as_str()).collect();
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
            + GPT5_NAMES.len()
            + THOUGHTS_NAMES.len()
    }
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
    fn total_tool_count_is_19() {
        assert_eq!(AgenticTools::total_tool_count(), 19);
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
        set.insert("".to_string());
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

        // Should have all 19 tools
        assert!(
            names.len() >= 19,
            "expected at least 19 tools, got {}",
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
            reg.contains("ask_reasoning_model"),
            "missing ask_reasoning_model from gpt5_reasoner"
        );
        assert!(
            reg.contains("thoughts_add_reference"),
            "missing thoughts_add_reference from thoughts_mcp_tools"
        );
    }

    #[test]
    fn allowlist_filters_to_specific_tools() {
        let mut set = HashSet::new();
        set.insert("cli_ls".to_string());
        set.insert("ask_reasoning_model".to_string());
        let config = AgenticToolsConfig {
            allowlist: Some(set),
            extras: serde_json::json!({}),
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
            extras: serde_json::json!({}),
        };

        let reg = AgenticTools::new(config);

        // Should find tools despite uppercase allowlist
        assert!(reg.contains("cli_ls"));
        assert!(reg.contains("ask_reasoning_model"));
    }

    #[test]
    fn empty_allowlist_returns_empty_registry() {
        let config = AgenticToolsConfig {
            allowlist: Some(HashSet::new()),
            extras: serde_json::json!({}),
        };

        let reg = AgenticTools::new(config);

        // Empty allowlist should result in all tools (empty = no filtering)
        // This matches the behavior in normalize_allowlist where empty sets
        // after filtering are effectively None
        assert!(reg.len() >= 19);
    }

    #[test]
    fn unknown_allowlist_names_are_ignored() {
        let mut set = HashSet::new();
        set.insert("cli_ls".to_string());
        set.insert("nonexistent_tool".to_string());
        let config = AgenticToolsConfig {
            allowlist: Some(set),
            extras: serde_json::json!({}),
        };

        let reg = AgenticTools::new(config);

        // Should only have "cli_ls", ignoring "nonexistent_tool"
        assert_eq!(reg.len(), 1);
        assert!(reg.contains("cli_ls"));
    }
}
