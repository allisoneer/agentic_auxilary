//! V2 health API.

#[derive(Clone)]
pub struct HealthApi {
    http: super::V2HttpClient,
}

impl HealthApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn get(&self) -> crate::error::Result<crate::types::v2::health::Health> {
        self.http.get("/api/health").await
    }
}
