use crate::error::Result;
use crate::http::HttpClient;
use crate::types::v2::agent::AgentInfo;
use crate::types::v2::envelope::LocationEnvelope;

#[derive(Clone)]
pub struct AgentApi {
    http: HttpClient,
}

impl AgentApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(&self) -> Result<LocationEnvelope<Vec<AgentInfo>>> {
        self.http.api_get("/api/agent").await
    }
}
