use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::v2::envelope::CursorEnvelope;
use crate::types::v2::envelope::DataEnvelope;
use crate::types::v2::message::Message;
use crate::types::v2::session::PromptAdmitted;
use crate::types::v2::session::PromptRequest;
use crate::types::v2::session::SessionInfo;
use crate::types::v2::session::SessionListQuery;
use reqwest::Method;

#[derive(Clone)]
pub struct SessionApi {
    http: HttpClient,
}

impl SessionApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(&self, query: &SessionListQuery) -> Result<CursorEnvelope<Vec<SessionInfo>>> {
        self.http
            .api_request_json_with_query(Method::GET, "/api/session", &query.to_query_pairs(), None)
            .await
    }

    pub async fn prompt(
        &self,
        session_id: &str,
        request: &PromptRequest,
    ) -> Result<DataEnvelope<PromptAdmitted>> {
        let session_id = encode_path_segment(session_id);
        self.http
            .api_post(&format!("/api/session/{session_id}/prompt"), request)
            .await
    }

    pub async fn wait(&self, session_id: &str) -> Result<()> {
        let session_id = encode_path_segment(session_id);
        self.http
            .api_request_empty(
                Method::POST,
                &format!("/api/session/{session_id}/wait"),
                None,
            )
            .await
    }

    pub async fn compact(&self, session_id: &str) -> Result<()> {
        let session_id = encode_path_segment(session_id);
        self.http
            .api_request_empty(
                Method::POST,
                &format!("/api/session/{session_id}/compact"),
                None,
            )
            .await
    }

    pub async fn context(&self, session_id: &str) -> Result<DataEnvelope<Vec<Message>>> {
        let session_id = encode_path_segment(session_id);
        self.http
            .api_get(&format!("/api/session/{session_id}/context"))
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
    use wiremock::matchers::body_json;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn parses_prompt_admitted_receipt() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/session/session-1/prompt"))
            .and(body_json(serde_json::json!({
                "prompt": {"text": "hello"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {"id": "input-1"}
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

        let response = api
            .prompt(
                "session-1",
                &PromptRequest {
                    id: None,
                    prompt: crate::types::v2::session::PromptInput {
                        text: "hello".to_string(),
                        files: Vec::new(),
                        agents: Vec::new(),
                    },
                    delivery: None,
                    resume: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(response.data.id.as_deref(), Some("input-1"));
    }

    #[tokio::test]
    async fn wait_accepts_no_content_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/session/session-1/wait"))
            .respond_with(ResponseTemplate::new(204))
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

        api.wait("session-1").await.unwrap();
    }
}
