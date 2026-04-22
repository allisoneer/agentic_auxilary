use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;

#[derive(Clone)]
pub struct TuiApi {
    http: HttpClient,
}

impl TuiApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }
    pub async fn env(&self) -> Result<serde_json::Value> {
        self.http.request_json(Method::GET, "/tui/env", None).await
    }
    pub async fn command(&self, req: &serde_json::Value) -> Result<bool> {
        self.http
            .request_json(Method::POST, "/tui/command", Some(req.clone()))
            .await
    }
}
