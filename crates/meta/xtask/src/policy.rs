//! Policy configuration parsing and validation.
//!
//! Parses tools/policy.toml and provides validation utilities.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;

/// Top-level policy configuration.
#[derive(Debug, Deserialize)]
pub struct Policy {
    pub enums: Enums,
    pub integrations: Integrations,
    pub paths: Paths,
    pub todos: TodoPolicy,
    pub release_plz: ReleasePlz,
}

/// Enum definitions for metadata validation.
#[derive(Debug, Deserialize)]
pub struct Enums {
    pub role: Vec<String>,
    pub family: Vec<String>,
    #[serde(default)]
    pub documentation: Option<BTreeMap<String, String>>,
}

/// Integration dependency rules.
#[derive(Debug, Deserialize)]
pub struct Integrations {
    pub mcp: Option<IntegrationRule>,
    pub logging: Option<IntegrationRule>,
    pub napi: Option<IntegrationRule>,
}

/// A single integration rule specifying required dependencies.
#[derive(Debug, Deserialize)]
pub struct IntegrationRule {
    /// At least one of these dependencies must be present.
    #[serde(default)]
    pub any_of: Vec<String>,
    /// All of these dependencies must be present.
    #[serde(default)]
    pub all_of: Vec<String>,
    /// Error message to display if validation fails.
    #[serde(default)]
    pub message: Option<String>,
}

/// Path constraint configuration.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    /// Whether to enforce path constraints.
    pub enforce: bool,
    /// Allowed path patterns (glob) for workspace crates.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Current allowed paths.
    #[serde(default)]
    pub current: Option<PathRules>,
    /// Target paths (for documentation).
    #[serde(default)]
    pub target: Option<PathRules>,
}

/// Path rules for constraints.
#[derive(Debug, Deserialize)]
pub struct PathRules {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub documentation: Option<String>,
}

/// Annotation enforcement policy (severity-tagged comments).
#[derive(Debug, Deserialize)]
pub struct TodoPolicy {
    /// Severities that are not allowed in HEAD (default: [0]).
    #[serde(default = "default_blocked_severities")]
    pub blocked_severities: Vec<u8>,

    /// Repo-relative path prefixes to exclude from scanning.
    #[serde(default)]
    pub ignore_paths: Vec<String>,
}

fn default_blocked_severities() -> Vec<u8> {
    vec![0]
}

/// Release-plz configuration.
#[derive(Debug, Deserialize)]
pub struct ReleasePlz {
    pub git_tag_format: String,
    pub publish_default: bool,
    #[serde(default)]
    pub overrides: BTreeMap<String, ReleaseOverride>,
}

/// Per-package release-plz overrides.
#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseOverride {
    #[serde(default)]
    pub git_tag_enable: Option<bool>,
    #[serde(default)]
    pub publish: Option<bool>,
}

impl Policy {
    /// Load policy from the default location (tools/policy.toml).
    pub fn load() -> Result<Self> {
        Self::load_from("tools/policy.toml")
    }

    /// Load policy from a specific path.
    pub fn load_from(path: &str) -> Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("Failed to read policy at {path}"))?;
        let policy: Policy =
            toml::from_str(&contents).with_context(|| format!("Failed to parse {path}"))?;
        Ok(policy)
    }

    /// Check if a role is valid according to policy.
    pub fn is_valid_role(&self, role: &str) -> bool {
        self.enums.role.iter().any(|r| r == role)
    }

    /// Check if a family is valid according to policy.
    pub fn is_valid_family(&self, family: &str) -> bool {
        self.enums.family.iter().any(|f| f == family)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_parsing() {
        let toml = r#"
[enums]
role = ["app", "lib"]
family = ["tools", "services"]

[integrations.mcp]
any_of = ["agentic-tools-mcp"]
message = "MCP required"

[paths]
enforce = false
allow = ["apps/*", "crates/*"]

[todos]
blocked_severities = [0]
ignore_paths = ["CLAUDE.md"]

[release_plz]
git_tag_format = "{{ name }}-v{{ version }}"
publish_default = true
"#;
        let policy: Policy = toml::from_str(toml).unwrap();
        assert!(policy.is_valid_role("app"));
        assert!(!policy.is_valid_role("invalid"));
        assert!(!policy.paths.enforce);
        assert_eq!(policy.paths.allow, vec!["apps/*", "crates/*"]);
        assert_eq!(policy.todos.blocked_severities, vec![0]);
        assert_eq!(policy.todos.ignore_paths, vec!["CLAUDE.md"]);
    }
}
