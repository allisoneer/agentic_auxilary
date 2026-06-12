//! V2 reference API.

#[derive(Clone)]
pub struct ReferenceApi {
    http: super::V2HttpClient,
}

impl ReferenceApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(
        &self,
    ) -> crate::error::Result<crate::types::v2::common::LocationResponse<Vec<serde_json::Value>>>
    {
        self.http.get("/api/reference").await
    }
}
