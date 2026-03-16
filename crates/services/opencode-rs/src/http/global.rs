//! Global API for `OpenCode`.
//!
//! Provides access to global SSE event streams across all directories.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::event::GlobalEventEnvelope;
use reqwest::Method;

/// Global API client.
///
/// This provides HTTP-level access to global endpoints, including
/// the global event stream metadata.
#[derive(Clone)]
pub struct GlobalApi {
    http: HttpClient,
}

impl GlobalApi {
    /// Create a new Global API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Get the base URL for the global event stream.
    ///
    /// Returns the URL for `/global/event` SSE endpoint.
    /// Use `Client::subscribe_global()` for actual SSE streaming.
    pub fn event_stream_url(&self) -> String {
        format!("{}/global/event", self.http.base())
    }

    /// Get the base URL for the directory-scoped event stream.
    ///
    /// Returns the URL for `/event` SSE endpoint.
    /// Use `Client::subscribe()` for actual SSE streaming.
    pub fn directory_event_stream_url(&self) -> String {
        format!("{}/event", self.http.base())
    }

    /// Check if the global endpoint is available (health check).
    ///
    /// This performs a quick GET request to verify connectivity.
    ///
    /// # Errors
    ///
    /// Returns an error if the server is unreachable.
    pub async fn health(&self) -> Result<bool> {
        use crate::http::misc::HealthInfo;
        let info: HealthInfo = self
            .http
            .request_json(Method::GET, "/global/health", None)
            .await?;
        Ok(info.healthy)
    }
}

/// Helper functions for parsing global event envelopes.
impl GlobalEventEnvelope {
    /// Check if this envelope is for a specific directory.
    pub fn is_directory(&self, dir: &str) -> bool {
        self.directory == dir
    }

    /// Check if the payload is a question event.
    pub fn is_question_event(&self) -> bool {
        matches!(
            &self.payload,
            crate::types::event::Event::QuestionAsked { .. }
                | crate::types::event::Event::QuestionReplied { .. }
                | crate::types::event::Event::QuestionRejected { .. }
        )
    }

    /// Check if the payload is a session event.
    pub fn is_session_event(&self) -> bool {
        matches!(
            &self.payload,
            crate::types::event::Event::SessionCreated { .. }
                | crate::types::event::Event::SessionUpdated { .. }
                | crate::types::event::Event::SessionDeleted { .. }
                | crate::types::event::Event::SessionError { .. }
                | crate::types::event::Event::SessionIdle { .. }
        )
    }

    /// Check if the payload is a message event.
    pub fn is_message_event(&self) -> bool {
        matches!(
            &self.payload,
            crate::types::event::Event::MessageUpdated { .. }
                | crate::types::event::Event::MessageRemoved { .. }
                | crate::types::event::Event::MessagePartUpdated { .. }
                | crate::types::event::Event::MessagePartRemoved { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use crate::types::event::{Event, QuestionAskedProps};
    use crate::types::question::QuestionRequest;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_global_api_event_stream_url() {
        let mock_server = MockServer::start().await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = GlobalApi::new(client);
        let url = api.event_stream_url();
        assert!(url.ends_with("/global/event"));
    }

    #[tokio::test]
    async fn test_global_api_directory_event_stream_url() {
        let mock_server = MockServer::start().await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: Some("/my/project".to_string()),
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = GlobalApi::new(client);
        let url = api.directory_event_stream_url();
        assert!(url.ends_with("/event"));
    }

    #[tokio::test]
    async fn test_global_api_health() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/global/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "healthy": true,
                "version": "1.0.0"
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = GlobalApi::new(client);
        let result = api.health().await.unwrap();
        assert!(result);
    }

    #[test]
    fn test_global_event_envelope_is_directory() {
        let envelope = GlobalEventEnvelope {
            directory: "/my/project".to_string(),
            payload: Event::ServerHeartbeat {
                properties: serde_json::Value::Null,
            },
        };
        assert!(envelope.is_directory("/my/project"));
        assert!(!envelope.is_directory("/other/project"));
    }

    #[test]
    fn test_global_event_envelope_is_question_event() {
        let envelope = GlobalEventEnvelope {
            directory: "/test".to_string(),
            payload: Event::QuestionAsked {
                properties: QuestionAskedProps {
                    request: QuestionRequest {
                        id: "q1".to_string(),
                        session_id: "s1".to_string(),
                        questions: vec![],
                        tool: None,
                        extra: serde_json::Value::Null,
                    },
                },
            },
        };
        assert!(envelope.is_question_event());
        assert!(!envelope.is_session_event());
        assert!(!envelope.is_message_event());
    }

    #[test]
    fn test_global_event_envelope_is_session_event() {
        let envelope = GlobalEventEnvelope {
            directory: "/test".to_string(),
            payload: Event::SessionIdle {
                properties: crate::types::event::SessionIdleProps {
                    session_id: "s1".to_string(),
                    extra: serde_json::Value::Null,
                },
            },
        };
        assert!(envelope.is_session_event());
        assert!(!envelope.is_question_event());
    }

    #[test]
    fn test_global_event_envelope_deserialize() {
        let json = r#"{
            "directory": "/project/path",
            "payload": {
                "type": "server.heartbeat",
                "properties": {}
            }
        }"#;
        let envelope: GlobalEventEnvelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.directory, "/project/path");
        assert!(matches!(envelope.payload, Event::ServerHeartbeat { .. }));
    }
}
