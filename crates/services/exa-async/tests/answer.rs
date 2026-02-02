use exa_async::types::answer::AnswerRequest;
use exa_async::{Client, ExaConfig};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_client(server: &MockServer) -> Client<ExaConfig> {
    let config = ExaConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test-api-key");
    Client::with_config(config)
}

#[tokio::test]
async fn answer_success_parses() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/answer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "answer": "The answer to your question is 42.",
            "citations": [
                {
                    "url": "https://example.com/source",
                    "title": "Source Page",
                    "id": "src-1"
                }
            ],
            "costDollars": {
                "total": 0.01
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = test_client(&server);
    let req = AnswerRequest::new("What is the meaning of life?").with_model("exa");
    let resp = client.answer().create(req).await.unwrap();

    assert_eq!(resp.answer, "The answer to your question is 42.");
    assert_eq!(resp.citations.len(), 1);
    assert_eq!(resp.citations[0].url, "https://example.com/source");
    assert_eq!(resp.citations[0].title.as_deref(), Some("Source Page"));
}

#[tokio::test]
async fn answer_request_serializes_camel_case() {
    let req = AnswerRequest::new("test query").with_model("exa-pro");

    let serialized = serde_json::to_value(req).unwrap();
    assert_eq!(serialized["query"], "test query");
    assert_eq!(serialized["model"], "exa-pro");
}
