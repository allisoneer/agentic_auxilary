use serde::{Deserialize, Serialize};

/// An Anthropic model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Model {
    /// Model identifier
    pub id: String,
    /// When the model was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Display name for the model
    pub display_name: String,
    /// Type of resource (always "model")
    #[serde(rename = "type")]
    pub kind: String,
}

/// Response from listing models
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelsListResponse {
    /// List of models
    pub data: Vec<Model>,
    /// Whether there are more models available
    pub has_more: bool,
    /// ID of the first model in the list
    pub first_id: Option<String>,
    /// ID of the last model in the list
    pub last_id: Option<String>,
}

/// Parameters for listing models
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelListParams {
    /// Return models after this ID (for pagination)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    /// Return models before this ID (for pagination)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    /// Maximum number of models to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}
