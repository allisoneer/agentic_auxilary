use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;

#[derive(Clone)]
pub struct ConsoleApi {
    http: HttpClient,
}

impl ConsoleApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }
    pub async fn list(&self) -> Result<Vec<serde_json::Value>> {
        self.http
            .request_json(Method::GET, "/experimental/console", None)
            .await
    }
    pub async fn create(&self, req: &serde_json::Value) -> Result<serde_json::Value> {
        self.http
            .request_json(Method::POST, "/experimental/console", Some(req.clone()))
            .await
    }
}
