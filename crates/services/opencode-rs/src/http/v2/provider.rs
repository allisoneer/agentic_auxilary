//! V2 provider API.

use crate::http::encode_path_segment;

#[derive(Clone)]
pub struct ProviderApi {
    http: super::V2HttpClient,
}

impl ProviderApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(
        &self,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<Vec<serde_json::Value>>>
    {
        self.http.get("/api/provider").await
    }

    pub async fn get(
        &self,
        provider_id: &str,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<serde_json::Value>> {
        let pid = encode_path_segment(provider_id);
        self.http.get(&format!("/api/provider/{pid}")).await
    }
}
