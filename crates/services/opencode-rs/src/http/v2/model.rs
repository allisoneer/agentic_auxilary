//! V2 model API.

#[derive(Clone)]
pub struct ModelApi {
    http: super::V2HttpClient,
}

impl ModelApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(
        &self,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<Vec<serde_json::Value>>>
    {
        self.http.get("/api/model").await
    }
}
