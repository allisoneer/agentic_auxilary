//! Messages API for `OpenCode`.
//!
//! This module provides methods for message endpoints (6 total).

use crate::error::Result;
use crate::http::{HttpClient, encode_path_segment};
use crate::types::api::{CommandResponse, PromptResponse, ShellResponse};
use crate::types::message::{CommandRequest, Message, PromptRequest, ShellRequest};
use reqwest::Method;

/// Messages API client.
#[derive(Clone)]
pub struct MessagesApi {
    http: HttpClient,
}

impl MessagesApi {
    /// Create a new Messages API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Send a prompt to a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn prompt(&self, session_id: &str, req: &PromptRequest) -> Result<PromptResponse> {
        let sid = encode_path_segment(session_id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, &format!("/session/{sid}/message"), Some(body))
            .await
    }

    /// List messages in a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self, session_id: &str) -> Result<Vec<Message>> {
        let sid = encode_path_segment(session_id);
        self.http
            .request_json(Method::GET, &format!("/session/{sid}/message"), None)
            .await
    }

    /// Get a specific message.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get(&self, session_id: &str, message_id: &str) -> Result<Message> {
        let sid = encode_path_segment(session_id);
        let mid = encode_path_segment(message_id);
        self.http
            .request_json(Method::GET, &format!("/session/{sid}/message/{mid}"), None)
            .await
    }

    /// Send a prompt asynchronously (returns immediately with 204 No Content).
    ///
    /// Unlike `prompt`, this endpoint returns immediately and the response
    /// is streamed via SSE events. The server returns HTTP 204 with no body.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn prompt_async(&self, session_id: &str, req: &PromptRequest) -> Result<()> {
        let sid = encode_path_segment(session_id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_empty(
                Method::POST,
                &format!("/session/{sid}/prompt_async"),
                Some(body),
            )
            .await
    }

    /// Execute a command in a session.
    ///
    /// Uses transport-level retry for transient network failures (connect/timeout).
    /// This is safe because command dispatch is idempotent at the session level.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails after retries.
    pub async fn command(&self, session_id: &str, req: &CommandRequest) -> Result<CommandResponse> {
        let sid = encode_path_segment(session_id);
        let body = serde_json::to_value(req)?;
        self.http
            .post_json_with_retry(&format!("/session/{sid}/command"), Some(body))
            .await
    }

    /// Execute a shell command in a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn shell(&self, session_id: &str, req: &ShellRequest) -> Result<ShellResponse> {
        let sid = encode_path_segment(session_id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, &format!("/session/{sid}/shell"), Some(body))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use crate::types::message::{CommandRequest, PromptPart, ShellRequest};
    use std::time::Duration;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_prompt() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/message"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let result = messages
            .prompt(
                "s1",
                &PromptRequest {
                    parts: vec![PromptPart::Text {
                        text: "Hello".to_string(),
                        synthetic: None,
                        ignored: None,
                        metadata: None,
                    }],
                    message_id: None,
                    model: None,
                    agent: None,
                    no_reply: None,
                    system: None,
                    variant: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.status, Some("ok".to_string()));
    }

    #[tokio::test]
    async fn test_list_messages() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/s1/message"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "info": {"id": "m1", "sessionId": "s1", "role": "user", "time": {"created": 1_234_567_890}},
                    "parts": []
                },
                {
                    "info": {"id": "m2", "sessionId": "s1", "role": "assistant", "time": {"created": 1_234_567_891}},
                    "parts": []
                }
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let list = messages.list("s1").await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].role(), "user");
        assert_eq!(list[1].role(), "assistant");
    }

    #[tokio::test]
    async fn test_prompt_async() {
        let mock_server = MockServer::start().await;

        // Server returns 204 No Content (fire-and-forget pattern)
        Mock::given(method("POST"))
            .and(path("/session/s1/prompt_async"))
            .and(body_json(serde_json::json!({
                "parts": [
                    { "type": "text", "text": "Hello async" }
                ]
            })))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        messages
            .prompt_async(
                "s1",
                &PromptRequest {
                    parts: vec![PromptPart::Text {
                        text: "Hello async".to_string(),
                        synthetic: None,
                        ignored: None,
                        metadata: None,
                    }],
                    message_id: None,
                    model: None,
                    agent: None,
                    no_reply: None,
                    system: None,
                    variant: None,
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_command() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/command"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "executed"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let result = messages
            .command(
                "s1",
                &CommandRequest {
                    command: "test_command".to_string(),
                    arguments: String::new(),
                },
            )
            .await
            .unwrap();
        assert_eq!(result.status, Some("executed".to_string()));
    }

    #[tokio::test]
    async fn test_shell() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/shell"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "running"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let result = messages
            .shell(
                "s1",
                &ShellRequest {
                    command: "echo hello".to_string(),
                    model: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.status, Some("running".to_string()));
    }

    // ==================== Error Case Tests ====================

    #[tokio::test]
    async fn test_prompt_session_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/missing/message"))
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

        let messages = MessagesApi::new(http);
        let result = messages
            .prompt(
                "missing",
                &PromptRequest {
                    parts: vec![PromptPart::Text {
                        text: "test".to_string(),
                        synthetic: None,
                        ignored: None,
                        metadata: None,
                    }],
                    message_id: None,
                    model: None,
                    agent: None,
                    no_reply: None,
                    system: None,
                    variant: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_prompt_validation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/message"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "name": "ValidationError",
                "message": "Invalid prompt format"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let result = messages
            .prompt(
                "s1",
                &PromptRequest {
                    parts: vec![],
                    message_id: None,
                    model: None,
                    agent: None,
                    no_reply: None,
                    system: None,
                    variant: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_validation_error());
    }

    #[tokio::test]
    async fn test_list_messages_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/missing/message"))
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

        let messages = MessagesApi::new(http);
        let result = messages.list("missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_get_message_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/s1/message/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Message not found"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let result = messages.get("s1", "missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_prompt_async_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/prompt_async"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "name": "InternalError",
                "message": "Failed to queue prompt"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let result = messages
            .prompt_async(
                "s1",
                &PromptRequest {
                    parts: vec![PromptPart::Text {
                        text: "test".to_string(),
                        synthetic: None,
                        ignored: None,
                        metadata: None,
                    }],
                    message_id: None,
                    model: None,
                    agent: None,
                    no_reply: None,
                    system: None,
                    variant: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_server_error());
    }

    #[tokio::test]
    async fn test_command_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/missing/command"))
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

        let messages = MessagesApi::new(http);
        let result = messages
            .command(
                "missing",
                &CommandRequest {
                    command: "test".to_string(),
                    arguments: String::new(),
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_shell_validation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/shell"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "name": "ValidationError",
                "message": "Invalid shell command"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let messages = MessagesApi::new(http);
        let result = messages
            .shell(
                "s1",
                &ShellRequest {
                    command: String::new(),
                    model: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_validation_error());
    }
}
