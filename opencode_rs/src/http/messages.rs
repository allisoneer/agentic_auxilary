//! Messages API for OpenCode.
//!
//! This module provides methods for message endpoints.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::message::{Message, PromptRequest};
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use crate::types::message::PromptPart;
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
                    }],
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
}
