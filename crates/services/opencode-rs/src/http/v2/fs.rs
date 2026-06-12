//! V2 filesystem API.

#[derive(Clone)]
pub struct FsApi {
    http: super::V2HttpClient,
}

impl FsApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(
        &self,
        path: Option<&str>,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<Vec<serde_json::Value>>>
    {
        let mut query = Vec::new();
        if let Some(path) = path {
            query.push(("path", path.to_string()));
        }
        self.http.get_with_query("/api/fs/list", &query).await
    }

    pub async fn find(
        &self,
        query_text: &str,
        entry_type: Option<&str>,
        limit: Option<u64>,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<Vec<serde_json::Value>>>
    {
        let mut query = vec![("query", query_text.to_string())];
        if let Some(entry_type) = entry_type {
            query.push(("type", entry_type.to_string()));
        }
        if let Some(limit) = limit {
            query.push(("limit", limit.to_string()));
        }
        self.http.get_with_query("/api/fs/find", &query).await
    }
}
