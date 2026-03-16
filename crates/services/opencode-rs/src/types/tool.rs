//! Tool and agent types for `opencode_rs`.

use crate::types::permission::Ruleset;
use crate::types::project::ModelRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Agent mode (how the agent operates).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Subagent mode (child agent).
    Subagent,
    /// Primary agent mode.
    Primary,
    /// Available in all contexts.
    All,
    /// Unknown mode (forward compatibility).
    #[serde(other)]
    Unknown,
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

    // ==================== Upstream parity fields ====================
    /// Agent mode (subagent, primary, all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<AgentMode>,

    /// Whether this is a native agent.
    #[serde(default)]
    pub native: bool,

    /// Whether this agent is hidden from UI.
    #[serde(default)]
    pub hidden: bool,

    /// Top-p sampling parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    /// Temperature sampling parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Agent color for UI display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Permission ruleset for this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<Ruleset>,

    /// Model reference for this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,

    /// Model variant name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,

    /// Prompt template for this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    /// Additional options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, serde_json::Value>,

    /// Maximum steps for this agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_mode_serialize() {
        assert_eq!(
            serde_json::to_string(&AgentMode::Subagent).unwrap(),
            r#""subagent""#
        );
        assert_eq!(
            serde_json::to_string(&AgentMode::Primary).unwrap(),
            r#""primary""#
        );
        assert_eq!(serde_json::to_string(&AgentMode::All).unwrap(), r#""all""#);
    }

    #[test]
    fn test_agent_mode_deserialize() {
        assert_eq!(
            serde_json::from_str::<AgentMode>(r#""subagent""#).unwrap(),
            AgentMode::Subagent
        );
        assert_eq!(
            serde_json::from_str::<AgentMode>(r#""primary""#).unwrap(),
            AgentMode::Primary
        );
        assert_eq!(
            serde_json::from_str::<AgentMode>(r#""all""#).unwrap(),
            AgentMode::All
        );
    }

    #[test]
    fn test_agent_mode_unknown() {
        // Unknown mode should deserialize as Unknown
        assert_eq!(
            serde_json::from_str::<AgentMode>(r#""future_mode""#).unwrap(),
            AgentMode::Unknown
        );
    }

    #[test]
    fn test_agent_minimal() {
        let json = r#"{"name": "coder"}"#;
        let agent: Agent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.name, "coder");
        assert!(agent.tools.is_empty());
        assert!(!agent.builtin);
        assert!(!agent.native);
        assert!(!agent.hidden);
        assert!(agent.mode.is_none());
    }

    #[test]
    fn test_agent_with_new_fields() {
        let json = r##"{
            "name": "custom-agent",
            "description": "A custom agent",
            "mode": "subagent",
            "native": true,
            "hidden": false,
            "topP": 0.9,
            "temperature": 0.7,
            "color": "#ff0000",
            "variant": "fast",
            "prompt": "You are helpful",
            "steps": 10,
            "tools": ["read", "write"]
        }"##;
        let agent: Agent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.name, "custom-agent");
        assert_eq!(agent.description, Some("A custom agent".to_string()));
        assert_eq!(agent.mode, Some(AgentMode::Subagent));
        assert!(agent.native);
        assert!(!agent.hidden);
        assert_eq!(agent.top_p, Some(0.9));
        assert_eq!(agent.temperature, Some(0.7));
        assert_eq!(agent.color, Some("#ff0000".to_string()));
        assert_eq!(agent.variant, Some("fast".to_string()));
        assert_eq!(agent.prompt, Some("You are helpful".to_string()));
        assert_eq!(agent.steps, Some(10));
        assert_eq!(agent.tools, vec!["read", "write"]);
    }

    #[test]
    fn test_agent_with_model_ref() {
        let json = r#"{
            "name": "model-agent",
            "model": {
                "providerId": "anthropic",
                "modelId": "claude-3-opus"
            }
        }"#;
        let agent: Agent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.name, "model-agent");
        let model = agent.model.unwrap();
        assert_eq!(model.provider_id, Some("anthropic".to_string()));
        assert_eq!(model.model_id, Some("claude-3-opus".to_string()));
    }

    #[test]
    fn test_agent_with_permission() {
        let json = r#"{
            "name": "restricted-agent",
            "permission": [
                {"permission": "file.read", "pattern": "**/*.rs", "action": "allow"}
            ]
        }"#;
        let agent: Agent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.name, "restricted-agent");
        let permission = agent.permission.unwrap();
        assert_eq!(permission.len(), 1);
        assert_eq!(permission[0].permission, "file.read");
    }

    #[test]
    fn test_agent_with_options() {
        let json = r#"{
            "name": "options-agent",
            "options": {
                "maxTokens": 1000,
                "verbose": true
            }
        }"#;
        let agent: Agent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.name, "options-agent");
        assert_eq!(agent.options.len(), 2);
        assert_eq!(agent.options["maxTokens"], serde_json::json!(1000));
        assert_eq!(agent.options["verbose"], serde_json::json!(true));
    }

    #[test]
    fn test_agent_extra_fields_preserved() {
        let json = r#"{
            "name": "future-agent",
            "futureField": "unknown value",
            "anotherFuture": 42
        }"#;
        let agent: Agent = serde_json::from_str(json).unwrap();
        assert_eq!(agent.name, "future-agent");
        assert_eq!(agent.extra["futureField"], "unknown value");
        assert_eq!(agent.extra["anotherFuture"], 42);
    }

    #[test]
    fn test_agent_round_trip() {
        let agent = Agent {
            name: "test-agent".to_string(),
            description: Some("Test agent".to_string()),
            system: Some("You are a test agent".to_string()),
            tools: vec!["read".to_string(), "write".to_string()],
            builtin: true,
            mode: Some(AgentMode::Primary),
            native: false,
            hidden: false,
            top_p: Some(0.95),
            temperature: Some(0.5),
            color: Some("#00ff00".to_string()),
            permission: None,
            model: Some(ModelRef {
                provider_id: Some("openai".to_string()),
                model_id: Some("gpt-4".to_string()),
                extra: serde_json::Value::Null,
            }),
            variant: Some("turbo".to_string()),
            prompt: None,
            options: HashMap::new(),
            steps: Some(5),
            extra: serde_json::Value::Null,
        };
        let json = serde_json::to_string(&agent).unwrap();
        let parsed: Agent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, agent.name);
        assert_eq!(parsed.mode, agent.mode);
        assert_eq!(parsed.top_p, agent.top_p);
    }
}
