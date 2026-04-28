use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;

#[derive(Clone)]
pub struct ExperimentalSessionApi {
    http: HttpClient,
}

impl ExperimentalSessionApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }
    pub async fn list(&self) -> Result<Vec<serde_json::Value>> {
        self.http
            .request_json(Method::GET, "/experimental/session", None)
            .await
    }
}
