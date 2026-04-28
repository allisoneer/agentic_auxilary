use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;

#[derive(Clone)]
pub struct WorkspacesApi {
    http: HttpClient,
}

impl WorkspacesApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }
    pub async fn list(&self) -> Result<Vec<serde_json::Value>> {
        self.http
            .request_json(Method::GET, "/experimental/workspace", None)
            .await
    }
    pub async fn current(&self) -> Result<serde_json::Value> {
        self.http
            .request_json(Method::GET, "/experimental/workspace/current", None)
            .await
    }
}
