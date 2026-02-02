use exa_async::types::contents::ContentsRequest;
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
async fn contents_success_parses() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/contents"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "url": "https://example.com/page1",
                    "title": "Example Page",
                    "text": "Full text content of the page.",
                    "summary": "A summary."
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = test_client(&server);
    let req = ContentsRequest::new(vec!["https://example.com/page1".into()]);
    let resp = client.contents().create(req).await.unwrap();

    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].url, "https://example.com/page1");
    assert_eq!(
        resp.results[0].text.as_deref(),
        Some("Full text content of the page.")
    );
}

#[tokio::test]
async fn contents_request_serializes_camel_case() {
    let req = ContentsRequest {
        urls: vec!["https://example.com".into()],
        contents: None,
        livecrawl: Some(exa_async::types::common::LivecrawlOption::Always),
        filter_empty_results: Some(true),
    };

    let serialized = serde_json::to_value(req).unwrap();
    assert!(serialized.get("filterEmptyResults").is_some());
    assert_eq!(serialized["livecrawl"], "always");
}
