//! PTY API for `OpenCode`.
//!
//! Endpoints for pseudo-terminal management.
//!
//! Note: Unit tests are intentionally skipped for this module because the
//! `GET /pty/{id}/connect` endpoint requires WebSocket support, which is
//! out of scope for this SDK version.

use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::pty::CreatePtyRequest;
use crate::types::pty::Pty;
use crate::types::pty::UpdatePtyRequest;
use reqwest::Method;

/// PTY API client.
#[derive(Clone)]
pub struct PtyApi {
    http: HttpClient,
}

impl PtyApi {
    /// Create a new PTY API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List PTYs.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<Pty>> {
        self.http.request_json(Method::GET, "/pty", None).await
    }

    /// Create a new PTY.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn create(&self, req: &CreatePtyRequest) -> Result<Pty> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/pty", Some(body))
            .await
    }

    /// Get a PTY by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get(&self, id: &str) -> Result<Pty> {
        let id = encode_path_segment(id);
        self.http
            .request_json(Method::GET, &format!("/pty/{id}"), None)
            .await
    }

    /// Update a PTY.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn update(&self, id: &str, req: &UpdatePtyRequest) -> Result<Pty> {
        let id = encode_path_segment(id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::PUT, &format!("/pty/{id}"), Some(body))
            .await
    }

    /// Delete a PTY.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let id = encode_path_segment(id);
        self.http
            .request_json::<bool>(Method::DELETE, &format!("/pty/{id}"), None)
            .await
    }

    /// Connect a PTY stream.
    pub async fn connect(&self, id: &str) -> Result<bool> {
        let id = encode_path_segment(id);
        self.http
            .request_json(Method::GET, &format!("/pty/{id}/connect"), None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn test_connect() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/pty/pty-1/connect"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let pty = PtyApi::new(http);
        let connected = pty.connect("pty-1").await.unwrap();
        assert!(connected);
    }
}
