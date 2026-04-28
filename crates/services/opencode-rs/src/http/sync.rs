use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;

#[derive(Clone)]
pub struct SyncApi {
    http: HttpClient,
}

impl SyncApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }
    pub async fn start(&self) -> Result<bool> {
        self.http
            .request_json(Method::POST, "/sync/start", Some(serde_json::json!({})))
            .await
    }
    pub async fn replay(&self, req: &serde_json::Value) -> Result<serde_json::Value> {
        self.http
            .request_json(Method::POST, "/sync/replay", Some(req.clone()))
            .await
    }
    pub async fn history(&self, req: &serde_json::Value) -> Result<Vec<serde_json::Value>> {
        self.http
            .request_json(Method::POST, "/sync/history", Some(req.clone()))
            .await
    }
}
