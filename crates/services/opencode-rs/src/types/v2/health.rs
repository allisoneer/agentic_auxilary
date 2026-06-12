//! V2 health endpoint types.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub healthy: bool,
}
