//! V2 connector API.

use crate::http::encode_path_segment;

#[derive(Clone)]
pub struct ConnectorApi {
    http: super::V2HttpClient,
}

impl ConnectorApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(
        &self,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<Vec<serde_json::Value>>>
    {
        self.http.get("/api/connector").await
    }

    pub async fn get(
        &self,
        connector_id: &str,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<Option<serde_json::Value>>>
    {
        let cid = encode_path_segment(connector_id);
        self.http.get(&format!("/api/connector/{cid}")).await
    }

    pub async fn connect_key(
        &self,
        connector_id: &str,
        req: &serde_json::Value,
    ) -> crate::error::Result<()> {
        let cid = encode_path_segment(connector_id);
        self.http
            .post_empty(&format!("/api/connector/{cid}/connect/key"), req)
            .await
    }

    pub async fn connect_oauth_begin(
        &self,
        connector_id: &str,
        req: &serde_json::Value,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<serde_json::Value>> {
        let cid = encode_path_segment(connector_id);
        self.http
            .post(&format!("/api/connector/{cid}/connect/oauth"), req)
            .await
    }

    pub async fn oauth_status(
        &self,
        attempt_id: &str,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<serde_json::Value>> {
        let aid = encode_path_segment(attempt_id);
        self.http.get(&format!("/api/connector/oauth/{aid}")).await
    }

    pub async fn oauth_complete(
        &self,
        attempt_id: &str,
        req: &serde_json::Value,
    ) -> crate::error::Result<()> {
        let aid = encode_path_segment(attempt_id);
        self.http
            .post_empty(&format!("/api/connector/oauth/{aid}/complete"), req)
            .await
    }

    pub async fn oauth_cancel(&self, attempt_id: &str) -> crate::error::Result<()> {
        let aid = encode_path_segment(attempt_id);
        self.http
            .delete_empty(&format!("/api/connector/oauth/{aid}"))
            .await
    }
}
