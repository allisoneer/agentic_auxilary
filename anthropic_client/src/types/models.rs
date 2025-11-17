use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub display_name: Option<String>,
    #[serde(rename = "type")]
    pub kind: String, // expected "model"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelsListResponse {
    pub data: Vec<Model>,
    pub has_more: Option<bool>,
    pub first_id: Option<String>,
    pub last_id: Option<String>,
}
