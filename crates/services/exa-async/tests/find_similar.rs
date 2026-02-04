use exa_async::types::find_similar::FindSimilarRequest;
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
async fn find_similar_success_parses() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/findSimilar"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [
                {
                    "url": "https://similar.com/page",
                    "title": "Similar Page",
                    "score": 0.88
                }
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = test_client(&server);
    let req = FindSimilarRequest::new("https://example.com")
        .with_num_results(5)
        .with_exclude_source_domain(true);
    let resp = client.find_similar().create(req).await.unwrap();

    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].url, "https://similar.com/page");
    assert!((resp.results[0].score.unwrap() - 0.88).abs() < f64::EPSILON);
}

#[tokio::test]
async fn find_similar_request_serializes_camel_case() {
    let req = FindSimilarRequest::new("https://example.com")
        .with_num_results(3)
        .with_exclude_source_domain(true);

    let serialized = serde_json::to_value(req).unwrap();
    assert_eq!(serialized["url"], "https://example.com");
    assert_eq!(serialized["numResults"], 3);
    assert_eq!(serialized["excludeSourceDomain"], true);
}
