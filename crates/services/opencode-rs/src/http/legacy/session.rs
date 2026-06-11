use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::api::CommandResponse;
use crate::types::message::CommandRequest;
use crate::types::session::CreateSessionRequest;
use crate::types::session::Session;
use reqwest::Method;

#[derive(Clone)]
pub struct SessionApi {
    http: HttpClient,
}

impl SessionApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// `LEGACY_EXCEPTION(OpenCode v1.17.2)`: upstream still lacks `/api/*` session creation.
    pub async fn create(&self, request: &CreateSessionRequest) -> Result<Session> {
        let body = serde_json::to_value(request)?;
        self.http
            .request_json(Method::POST, "/session", Some(body))
            .await
    }

    /// `LEGACY_EXCEPTION(OpenCode v1.17.2)`: upstream still lacks `/api/*` session fetch-by-id.
    pub async fn get(&self, session_id: &str) -> Result<Session> {
        let session_id = encode_path_segment(session_id);
        self.http
            .request_json(Method::GET, &format!("/session/{session_id}"), None)
            .await
    }

    /// `LEGACY_EXCEPTION(OpenCode v1.17.2)`: upstream still lacks `/api/*` command execution.
    pub async fn command(
        &self,
        session_id: &str,
        request: &CommandRequest,
    ) -> Result<CommandResponse> {
        let session_id = encode_path_segment(session_id);
        let body = serde_json::to_value(request)?;
        self.http
            .post_json_with_retry(&format!("/session/{session_id}/command"), Some(body))
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
    async fn supports_legacy_session_create_and_get_and_command() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "session-1",
                "slug": "session-1"
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/session/session-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "session-1",
                "slug": "session-1"
            })))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/session/session-1/command"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "queued"
            })))
            .mount(&mock_server)
            .await;

        let api = SessionApi::new(
            HttpClient::new(HttpConfig {
                base_url: mock_server.uri(),
                directory: None,
                workspace: None,
                timeout: Duration::from_secs(30),
            })
            .unwrap(),
        );

        let created = api.create(&CreateSessionRequest::default()).await.unwrap();
        assert_eq!(created.id, "session-1");

        let fetched = api.get("session-1").await.unwrap();
        assert_eq!(fetched.id, "session-1");

        let command = api
            .command(
                "session-1",
                &CommandRequest {
                    command: "research".to_string(),
                    arguments: "topic".to_string(),
                    message_id: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(command.status.as_deref(), Some("queued"));
    }
}
