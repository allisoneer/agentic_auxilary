//! Sessions API for OpenCode.
//!
//! This module provides methods for session endpoints (18 total).

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::session::{
    CreateSessionRequest, RevertRequest, Session, SessionDiff, SessionStatus, ShareInfo,
    SummarizeRequest, TodoItem, UpdateSessionRequest,
};
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
    pub async fn status(&self) -> Result<SessionStatus> {
        self.http
            .request_json(Method::GET, "/session/status", None)
            .await
    }

    /// Get children of a session (forked sessions).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn children(&self, id: &str) -> Result<Vec<Session>> {
        self.http
            .request_json(Method::GET, &format!("/session/{}/children", id), None)
            .await
    }

    /// Get todos for a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn todo(&self, id: &str) -> Result<Vec<TodoItem>> {
        self.http
            .request_json(Method::GET, &format!("/session/{}/todo", id), None)
            .await
    }

    /// Update a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn update(&self, id: &str, req: &UpdateSessionRequest) -> Result<Session> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::PATCH, &format!("/session/{}", id), Some(body))
            .await
    }

    /// Initialize a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn init(&self, id: &str) -> Result<Session> {
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/init", id),
                Some(serde_json::json!({})),
            )
            .await
    }

    /// Share a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn share(&self, id: &str) -> Result<ShareInfo> {
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/share", id),
                Some(serde_json::json!({})),
            )
            .await
    }

    /// Unshare a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn unshare(&self, id: &str) -> Result<()> {
        self.http
            .request_empty(Method::DELETE, &format!("/session/{}/share", id), None)
            .await
    }

    /// Get session diff.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn diff(&self, id: &str) -> Result<SessionDiff> {
        self.http
            .request_json(Method::GET, &format!("/session/{}/diff", id), None)
            .await
    }

    /// Get session diff since a specific message.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn diff_since_message(&self, id: &str, message_id: &str) -> Result<SessionDiff> {
        let encoded = urlencoding::encode(message_id);
        self.http
            .request_json(
                Method::GET,
                &format!("/session/{}/diff?messageID={}", id, encoded),
                None,
            )
            .await
    }

    /// Summarize a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn summarize(&self, id: &str, req: &SummarizeRequest) -> Result<Session> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/summarize", id),
                Some(body),
            )
            .await
    }

    /// Revert a session to a previous state.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn revert(&self, id: &str, req: &RevertRequest) -> Result<Session> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, &format!("/session/{}/revert", id), Some(body))
            .await
    }

    /// Unrevert a session (undo a revert).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn unrevert(&self, id: &str) -> Result<Session> {
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/unrevert", id),
                Some(serde_json::json!({})),
            )
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

    #[tokio::test]
    async fn test_children() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/parent123/children"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "child1", "projectId": "p1", "directory": "/path", "title": "Child 1", "version": "1.0", "time": {"created": 1234567890, "updated": 1234567890}}
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
        let children = sessions.children("parent123").await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, "child1");
    }

    #[tokio::test]
    async fn test_todo() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/s1/todo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "t1", "content": "Task 1", "completed": false},
                {"id": "t2", "content": "Task 2", "completed": true}
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
        let todos = sessions.todo("s1").await.unwrap();
        assert_eq!(todos.len(), 2);
        assert!(!todos[0].completed);
        assert!(todos[1].completed);
    }

    #[tokio::test]
    async fn test_update_session() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/session/s1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "s1",
                "projectId": "p1",
                "directory": "/path",
                "title": "Updated Title",
                "version": "1.0",
                "time": {"created": 1234567890, "updated": 1234567891}
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
            .update(
                "s1",
                &UpdateSessionRequest {
                    title: Some("Updated Title".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(session.title, "Updated Title");
    }

    #[tokio::test]
    async fn test_share() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/share"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "url": "https://share.example.com/s1"
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
        let share = sessions.share("s1").await.unwrap();
        assert_eq!(share.url, "https://share.example.com/s1");
    }

    #[tokio::test]
    async fn test_unshare() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/session/s1/share"))
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
        sessions.unshare("s1").await.unwrap();
    }

    #[tokio::test]
    async fn test_diff() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/s1/diff"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "diff": "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new",
                "files": ["file.rs"]
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
        let diff = sessions.diff("s1").await.unwrap();
        assert!(diff.diff.contains("file.rs"));
        assert_eq!(diff.files.len(), 1);
    }

    #[tokio::test]
    async fn test_summarize() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/summarize"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "s1",
                "projectId": "p1",
                "directory": "/path",
                "title": "Summarized Session",
                "version": "1.0",
                "time": {"created": 1234567890, "updated": 1234567891}
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
            .summarize(
                "s1",
                &SummarizeRequest {
                    provider_id: "anthropic".into(),
                    model_id: "claude-3-5-sonnet".into(),
                    auto: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(session.id, "s1");
    }

    #[tokio::test]
    async fn test_revert() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/revert"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "s1",
                "projectId": "p1",
                "directory": "/path",
                "title": "Reverted Session",
                "version": "1.0",
                "time": {"created": 1234567890, "updated": 1234567891}
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
            .revert(
                "s1",
                &crate::types::session::RevertRequest {
                    message_id: "m5".into(),
                    part_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(session.id, "s1");
    }

    #[tokio::test]
    async fn test_unrevert() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/unrevert"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "s1",
                "projectId": "p1",
                "directory": "/path",
                "title": "Unreverted Session",
                "version": "1.0",
                "time": {"created": 1234567890, "updated": 1234567891}
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
        let session = sessions.unrevert("s1").await.unwrap();
        assert_eq!(session.id, "s1");
    }

    // ==================== Error Case Tests ====================

    #[tokio::test]
    async fn test_get_session_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/nonexistent"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Session not found",
                "data": {"id": "nonexistent"}
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
        let result = sessions.get("nonexistent").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_not_found());
        assert_eq!(err.error_name(), Some("NotFound"));
    }

    #[tokio::test]
    async fn test_create_session_validation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "name": "ValidationError",
                "message": "Invalid session configuration"
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
        let result = sessions.create(&CreateSessionRequest::default()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_validation_error());
    }

    #[tokio::test]
    async fn test_children_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/missing/children"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Session not found"
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
        let result = sessions.children("missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_update_session_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/session/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Session not found"
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
        let result = sessions
            .update("missing", &UpdateSessionRequest { title: None })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_share_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/share"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "name": "InternalError",
                "message": "Failed to generate share link"
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
        let result = sessions.share("s1").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_server_error());
    }

    #[tokio::test]
    async fn test_diff_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/missing/diff"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Session not found"
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
        let result = sessions.diff("missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_summarize_validation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/summarize"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "name": "ValidationError",
                "message": "Invalid provider or model"
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
        let result = sessions
            .summarize(
                "s1",
                &SummarizeRequest {
                    provider_id: "invalid".into(),
                    model_id: "invalid".into(),
                    auto: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_validation_error());
    }

    #[tokio::test]
    async fn test_revert_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/missing/revert"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Session not found"
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
        let result = sessions
            .revert(
                "missing",
                &crate::types::session::RevertRequest {
                    message_id: "m1".into(),
                    part_id: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_abort_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/abort"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "name": "InternalError",
                "message": "Failed to abort session"
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
        let result = sessions.abort("s1").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_server_error());
    }

    #[tokio::test]
    async fn test_todo_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/missing/todo"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Session not found"
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
        let result = sessions.todo("missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }
}
