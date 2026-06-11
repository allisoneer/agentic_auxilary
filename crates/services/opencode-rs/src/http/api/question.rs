use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::v2::envelope::LocationEnvelope;
use crate::types::v2::question::QuestionReply;
use crate::types::v2::question::QuestionRequest;
use reqwest::Method;

#[derive(Clone)]
pub struct QuestionApi {
    http: HttpClient,
}

impl QuestionApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn list_requests(&self) -> Result<LocationEnvelope<Vec<QuestionRequest>>> {
        self.http.api_get("/api/question/request").await
    }

    pub async fn reply(
        &self,
        session_id: &str,
        request_id: &str,
        request: &QuestionReply,
    ) -> Result<()> {
        let session_id = encode_path_segment(session_id);
        let request_id = encode_path_segment(request_id);
        self.http
            .api_post_empty(
                &format!("/api/session/{session_id}/question/{request_id}/reply"),
                request,
            )
            .await
    }

    pub async fn reject(&self, session_id: &str, request_id: &str) -> Result<()> {
        let session_id = encode_path_segment(session_id);
        let request_id = encode_path_segment(request_id);
        self.http
            .api_request_empty(
                Method::POST,
                &format!("/api/session/{session_id}/question/{request_id}/reject"),
                None,
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
    async fn parses_location_wrapped_question_requests() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/question/request"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "location": {"directory": "/tmp/project"},
                "data": [
                    {
                        "id": "question-1",
                        "sessionID": "session-1",
                        "questions": [{"question": "Continue?"}]
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let api = QuestionApi::new(
            HttpClient::new(HttpConfig {
                base_url: mock_server.uri(),
                directory: None,
                workspace: None,
                timeout: Duration::from_secs(30),
            })
            .unwrap(),
        );

        let response = api.list_requests().await.unwrap();
        assert_eq!(response.location.directory, "/tmp/project");
        assert_eq!(response.data[0].id, "question-1");
        assert_eq!(response.data[0].session_id, "session-1");
    }
}
