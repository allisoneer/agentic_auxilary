use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInfo {
    #[serde(default)]
    pub directory: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "workspaceID"
    )]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
