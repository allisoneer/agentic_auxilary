use crate::error::Result;
use crate::http::HttpClient;
use crate::types::v2::command::CommandInfo;
use crate::types::v2::envelope::LocationEnvelope;

#[derive(Clone)]
pub struct CommandApi {
    http: HttpClient,
}

impl CommandApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(&self) -> Result<LocationEnvelope<Vec<CommandInfo>>> {
        self.http.api_get("/api/command").await
    }
}
