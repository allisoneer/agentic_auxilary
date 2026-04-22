//! Question API for `OpenCode`.
//!
//! Endpoints for managing question-answer flows.

use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::question::QuestionReply;
use crate::types::question::QuestionRequest;
use reqwest::Method;

/// Question API client.
#[derive(Clone)]
pub struct QuestionApi {
    http: HttpClient,
}

impl QuestionApi {
    /// Create a new Question API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List pending question requests.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<QuestionRequest>> {
        self.http.request_json(Method::GET, "/question", None).await
    }

    /// Reply to a question request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn reply(&self, request_id: &str, reply: &QuestionReply) -> Result<bool> {
        let rid = encode_path_segment(request_id);
        let body = serde_json::to_value(reply)?;
        self.http
            .request_json(Method::POST, &format!("/question/{rid}/reply"), Some(body))
            .await
    }

    /// Reject a question request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn reject(&self, request_id: &str) -> Result<bool> {
        let rid = encode_path_segment(request_id);
        self.http
            .request_json(
                Method::POST,
                &format!("/question/{rid}/reject"),
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
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn test_list_questions() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "req-123",
                    "sessionID": "sess-456",
                    "questions": [
                        {"question": "Continue?", "header": "Confirm"}
                    ]
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = QuestionApi::new(client);
        let questions = api.list().await.unwrap();
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].id, "req-123");
    }

    #[tokio::test]
    async fn test_reply_question() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/question/req-123/reply"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = QuestionApi::new(client);
        let reply = QuestionReply {
            answers: vec![vec!["Yes".to_string()]],
        };
        let result = api.reply("req-123", &reply).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_reject_question() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/question/req-456/reject"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = QuestionApi::new(client);
        let result = api.reject("req-456").await.unwrap();
        assert!(result);
    }
}
