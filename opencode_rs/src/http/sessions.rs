//! Sessions API for OpenCode.
//!
//! This module provides methods for session endpoints.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::session::{CreateSessionRequest, Session};
use reqwest::Method;

/// Sessions API client.
#[derive(Clone)]
pub struct SessionsApi {
    http: HttpClient,
}

impl SessionsApi {
    /// Create a new Sessions API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Create a new session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn create(&self, req: &CreateSessionRequest) -> Result<Session> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/session", Some(body))
            .await
    }

    /// Get session by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or session not found.
    pub async fn get(&self, id: &str) -> Result<Session> {
        self.http
            .request_json(Method::GET, &format!("/session/{}", id), None)
            .await
    }

    /// List all sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<Session>> {
        self.http.request_json(Method::GET, "/session", None).await
    }

    /// Delete session by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.http
            .request_empty(Method::DELETE, &format!("/session/{}", id), None)
            .await
    }

    /// Fork a session from a specific point.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn fork(&self, id: &str) -> Result<Session> {
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/fork", id),
                Some(serde_json::json!({})),
            )
            .await
    }

    /// Abort an active session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn abort(&self, id: &str) -> Result<()> {
        self.http
            .request_empty(
                Method::POST,
                &format!("/session/{}/abort", id),
                Some(serde_json::json!({})),
            )
            .await
    }

    /// Get session status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn status(&self) -> Result<serde_json::Value> {
        self.http
            .request_json(Method::GET, "/session/status", None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_create_session() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session"))
            .and(body_json(serde_json::json!({})))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "session123",
                "projectId": "proj1",
                "directory": "/path",
                "title": "New Session",
                "version": "1.0",
                "time": {"created": 1234567890, "updated": 1234567890}
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let sessions = SessionsApi::new(http);
        let session = sessions
            .create(&CreateSessionRequest::default())
            .await
            .unwrap();
        assert_eq!(session.id, "session123");
    }

    #[tokio::test]
    async fn test_get_session() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/abc123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "abc123",
                "projectId": "p1",
                "directory": "/path",
                "title": "Test Session",
                "version": "1.0",
                "time": {"created": 1234567890, "updated": 1234567890}
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let sessions = SessionsApi::new(http);
        let session = sessions.get("abc123").await.unwrap();
        assert_eq!(session.id, "abc123");
        assert_eq!(session.title, "Test Session");
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "s1", "projectId": "p1", "directory": "/path", "title": "S1", "version": "1.0", "time": {"created": 1234567890, "updated": 1234567890}},
                {"id": "s2", "projectId": "p1", "directory": "/path", "title": "S2", "version": "1.0", "time": {"created": 1234567890, "updated": 1234567890}}
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let sessions = SessionsApi::new(http);
        let list = sessions.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/session/del123"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let sessions = SessionsApi::new(http);
        sessions.delete("del123").await.unwrap();
    }
}
