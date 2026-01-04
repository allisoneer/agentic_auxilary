//! Messages API for OpenCode.
//!
//! This module provides methods for message endpoints (6 total).

use crate::error::Result;
use crate::http::HttpClient;
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
    pub async fn prompt(&self, session_id: &str, req: &PromptRequest) -> Result<serde_json::Value> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/message", session_id),
                Some(body),
            )
            .await
    }

    /// List messages in a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self, session_id: &str) -> Result<Vec<Message>> {
        self.http
            .request_json(
                Method::GET,
                &format!("/session/{}/message", session_id),
                None,
            )
            .await
    }

    /// Get a specific message.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get(&self, session_id: &str, message_id: &str) -> Result<Message> {
        self.http
            .request_json(
                Method::GET,
                &format!("/session/{}/message/{}", session_id, message_id),
                None,
            )
            .await
    }

    /// Send a prompt asynchronously (returns immediately).
    ///
    /// Unlike `prompt`, this endpoint returns immediately and the response
    /// is streamed via SSE events.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn prompt_async(
        &self,
        session_id: &str,
        req: &PromptRequest,
    ) -> Result<serde_json::Value> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/prompt_async", session_id),
                Some(body),
            )
            .await
    }

    /// Execute a command in a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn command(
        &self,
        session_id: &str,
        req: &CommandRequest,
    ) -> Result<serde_json::Value> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/command", session_id),
                Some(body),
            )
            .await
    }

    /// Execute a shell command in a session.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn shell(&self, session_id: &str, req: &ShellRequest) -> Result<serde_json::Value> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/session/{}/shell", session_id),
                Some(body),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use crate::types::message::{CommandRequest, PromptPart, ShellRequest};
    use std::time::Duration;
    use wiremock::matchers::{method, path};
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
        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn test_list_messages() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/s1/message"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "m1", "role": "user", "parts": []},
                {"id": "m2", "role": "assistant", "parts": []}
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
        assert_eq!(list[0].role, "user");
        assert_eq!(list[1].role, "assistant");
    }

    #[tokio::test]
    async fn test_prompt_async() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session/s1/prompt_async"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messageId": "m123"
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
        assert_eq!(result["messageId"], "m123");
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
                    args: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(result["status"], "executed");
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
        assert_eq!(result["status"], "running");
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
                    args: None,
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
                    command: "".to_string(),
                    model: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_validation_error());
    }
}
