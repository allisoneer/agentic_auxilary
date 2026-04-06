//! Parts API for `OpenCode`.
//!
//! Endpoints for modifying message parts.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::api::UpdatePartResponse;
use reqwest::Method;

/// Parts API client.
#[derive(Clone)]
pub struct PartsApi {
    http: HttpClient,
}

impl PartsApi {
    /// Create a new Parts API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Delete a part from a message.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete(&self, session_id: &str, message_id: &str, part_id: &str) -> Result<()> {
        self.http
            .request_empty(
                Method::DELETE,
                &format!("/session/{session_id}/message/{message_id}/part/{part_id}"),
                None,
            )
            .await
    }

    /// Update a part in a message.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn update(
        &self,
        session_id: &str,
        message_id: &str,
        part_id: &str,
        body: &serde_json::Value,
    ) -> Result<UpdatePartResponse> {
        self.http
            .request_json(
                Method::PATCH,
                &format!("/session/{session_id}/message/{message_id}/part/{part_id}"),
                Some(body.clone()),
            )
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
    async fn test_delete_part_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/session/s1/message/m1/part/p1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let parts = PartsApi::new(http);
        let result = parts.delete("s1", "m1", "p1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_part_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/session/s1/message/m1/part/p1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "p1",
                "type": "text",
                "content": "Updated content"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let parts = PartsApi::new(http);
        let result = parts
            .update(
                "s1",
                "m1",
                "p1",
                &serde_json::json!({
                    "content": "Updated content"
                }),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_part_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/session/s1/message/m1/part/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Part not found"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let parts = PartsApi::new(http);
        let result = parts.delete("s1", "m1", "missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }
}
