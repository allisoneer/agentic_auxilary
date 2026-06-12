//! V2 location API.

#[derive(Clone)]
pub struct LocationApi {
    http: super::V2HttpClient,
}

impl LocationApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn get(&self) -> crate::error::Result<crate::types::v2::location::Location> {
        self.http.get("/api/location").await
    }
}
