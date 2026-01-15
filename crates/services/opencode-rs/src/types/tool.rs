//! Tool and agent types for opencode_rs.

use serde::{Deserialize, Serialize};

/// A tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Tool identifier.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Tool description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Input schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    /// Whether this tool requires approval.
    #[serde(default)]
    pub requires_approval: bool,
    /// Source of the tool (builtin, mcp, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// An agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// Agent name.
    pub name: String,
    /// Agent description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// System prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Allowed tools.
    #[serde(default)]
    pub tools: Vec<String>,
    /// Whether this is a built-in agent.
    #[serde(default)]
    pub builtin: bool,
}

/// A command definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    /// Command name.
    pub name: String,
    /// Command description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Command shortcut key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
}

/// List of tool IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolIds {
    /// Tool identifiers.
    #[serde(default)]
    pub ids: Vec<String>,
}
