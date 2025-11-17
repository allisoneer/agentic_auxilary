use serde::{Deserialize, Serialize};

/// An Anthropic model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Model {
    /// Model identifier
    pub id: String,
    /// When the model was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Optional display name for the model
    pub display_name: Option<String>,
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
    pub has_more: Option<bool>,
    /// ID of the first model in the list
    pub first_id: Option<String>,
    /// ID of the last model in the list
    pub last_id: Option<String>,
}
