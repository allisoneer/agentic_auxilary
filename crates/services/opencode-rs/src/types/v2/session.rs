use super::message::Message;
use crate::types::project::ModelRef;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacting: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionSummary {
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<SessionTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SessionSummary>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl SessionListQuery {
    pub fn to_query_pairs(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();

        if let Some(directory) = &self.directory {
            query.push(("directory", directory.clone()));
        }
        if let Some(project) = &self.project {
            query.push(("project", project.clone()));
        }
        if let Some(subpath) = &self.subpath {
            query.push(("subpath", subpath.clone()));
        }
        if let Some(limit) = self.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(order) = &self.order {
            query.push(("order", order.clone()));
        }
        if let Some(search) = &self.search {
            query.push(("search", search.clone()));
        }
        if let Some(cursor) = &self.cursor {
            query.push(("cursor", cursor.clone()));
        }

        query
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptInput {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub prompt: PromptInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptAdmitted {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

pub type SessionContext = Vec<Message>;
