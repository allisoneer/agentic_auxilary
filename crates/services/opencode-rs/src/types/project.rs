//! Project types for `opencode_rs`.

use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

/// A project in `OpenCode`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// Project identifier.
    pub id: String,

    /// Project name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Project directory path.
    #[serde(alias = "path", default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,

    /// Whether this is the current project.
    #[serde(default)]
    pub current: bool,

    /// Project settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProjectSettings>,

    // ==================== Upstream parity fields ====================
    /// Worktree path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,

    /// Version control system (e.g., "git").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcs: Option<String>,

    /// Project icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<ProjectIcon>,

    /// Project commands.
    #[serde(default)]
    pub commands: ProjectCommands,

    /// Project timestamps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<ProjectTime>,

    /// Associated sandbox identifiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sandboxes: Vec<String>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Project icon definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIcon {
    /// Icon type (e.g., "emoji", "url", "lucide").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,

    /// Icon value (emoji character, URL, or icon name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// Additional icon properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Project commands configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCommands {
    /// Commands by name.
    #[serde(flatten)]
    pub commands: HashMap<String, serde_json::Value>,
}

/// Project timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTime {
    /// Creation timestamp (Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,

    /// Last update timestamp (Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<u64>,

    /// Last accessed timestamp (Unix epoch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessed: Option<u64>,

    /// Additional time properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Project settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    /// Default model for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelRef>,
    /// Default agent for this project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    /// Additional project-specific settings.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// TODO(3): Derive PartialEq on ModelRef, Project, ProjectSettings, UpdateProjectRequest for testing convenience
/// Reference to a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Provider identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Model identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Request to update a project.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectRequest {
    /// New project name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New project settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ProjectSettings>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_minimal() {
        let json = r#"{"id": "proj-123"}"#;
        let project: Project = serde_json::from_str(json).unwrap();
        assert_eq!(project.id, "proj-123");
        assert!(project.name.is_none());
        assert!(!project.current);
        assert!(project.sandboxes.is_empty());
    }

    #[test]
    fn test_project_with_new_fields() {
        let json = r#"{
            "id": "proj-123",
            "name": "My Project",
            "worktree": "/path/to/worktree",
            "vcs": "git",
            "icon": {
                "type": "emoji",
                "value": "rocket"
            },
            "time": {
                "created": 1234567890,
                "updated": 1234567891,
                "accessed": 1234567892
            },
            "sandboxes": ["sandbox-1", "sandbox-2"]
        }"#;
        let project: Project = serde_json::from_str(json).unwrap();
        assert_eq!(project.id, "proj-123");
        assert_eq!(project.name, Some("My Project".to_string()));
        assert_eq!(project.worktree, Some("/path/to/worktree".to_string()));
        assert_eq!(project.vcs, Some("git".to_string()));

        let icon = project.icon.unwrap();
        assert_eq!(icon.r#type, Some("emoji".to_string()));
        assert_eq!(icon.value, Some("rocket".to_string()));

        let time = project.time.unwrap();
        assert_eq!(time.created, Some(1_234_567_890));
        assert_eq!(time.updated, Some(1_234_567_891));
        assert_eq!(time.accessed, Some(1_234_567_892));

        assert_eq!(project.sandboxes, vec!["sandbox-1", "sandbox-2"]);
    }

    #[test]
    fn test_project_icon() {
        let json = r#"{"type": "url", "value": "https://example.com/icon.png"}"#;
        let icon: ProjectIcon = serde_json::from_str(json).unwrap();
        assert_eq!(icon.r#type, Some("url".to_string()));
        assert_eq!(icon.value, Some("https://example.com/icon.png".to_string()));
    }

    #[test]
    fn test_project_time() {
        let json = r#"{"created": 1000, "updated": 2000}"#;
        let time: ProjectTime = serde_json::from_str(json).unwrap();
        assert_eq!(time.created, Some(1000));
        assert_eq!(time.updated, Some(2000));
        assert!(time.accessed.is_none());
    }

    #[test]
    fn test_project_commands_empty() {
        let json = r"{}";
        let commands: ProjectCommands = serde_json::from_str(json).unwrap();
        assert!(commands.commands.is_empty());
    }

    #[test]
    fn test_project_commands_with_data() {
        let json = r#"{"build": {"script": "cargo build"}, "test": {"script": "cargo test"}}"#;
        let commands: ProjectCommands = serde_json::from_str(json).unwrap();
        assert_eq!(commands.commands.len(), 2);
        assert!(commands.commands.contains_key("build"));
        assert!(commands.commands.contains_key("test"));
    }

    #[test]
    fn test_project_extra_fields_preserved() {
        let json = r#"{
            "id": "proj-123",
            "futureField": "some value",
            "anotherFuture": 42
        }"#;
        let project: Project = serde_json::from_str(json).unwrap();
        assert_eq!(project.id, "proj-123");
        assert_eq!(project.extra["futureField"], "some value");
        assert_eq!(project.extra["anotherFuture"], 42);
    }

    #[test]
    fn test_model_ref() {
        let json = r#"{"providerId": "anthropic", "modelId": "claude-3"}"#;
        let model_ref: ModelRef = serde_json::from_str(json).unwrap();
        assert_eq!(model_ref.provider_id, Some("anthropic".to_string()));
        assert_eq!(model_ref.model_id, Some("claude-3".to_string()));
    }

    #[test]
    fn test_model_ref_partial() {
        // Server may send partial model refs
        let json = r"{}";
        let model_ref: ModelRef = serde_json::from_str(json).unwrap();
        assert!(model_ref.provider_id.is_none());
        assert!(model_ref.model_id.is_none());
    }
}
