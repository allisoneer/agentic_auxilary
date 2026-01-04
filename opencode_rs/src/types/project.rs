//! Project types for opencode_rs.

use serde::{Deserialize, Serialize};

/// A project in OpenCode.
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
    /// Additional fields from server.
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

/// Reference to a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Provider identifier.
    pub provider_id: String,
    /// Model identifier.
    pub model_id: String,
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
