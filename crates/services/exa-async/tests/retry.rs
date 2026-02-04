use exa_async::types::search::SearchRequest;
use exa_async::{Client, ExaConfig};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_client_fast_retry(server: &MockServer) -> Client<ExaConfig> {
    let config = ExaConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test-api-key");
    Client::with_config(config).with_backoff(
        backon::ExponentialBuilder::default()
            .with_min_delay(std::time::Duration::from_millis(10))
            .with_max_delay(std::time::Duration::from_millis(50))
            .with_max_times(3),
    )
}

#[tokio::test]
async fn retry_429_then_success() {
    let server = MockServer::start().await;

    // First request returns 429, second returns success
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "statusCode": 429,
            "message": "Rate limit exceeded"
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{"url": "https://example.com", "title": "Test"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = test_client_fast_retry(&server);
    let req = SearchRequest::new("test");
    let resp = client.search().create(req).await.unwrap();

    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].url, "https://example.com");
}

#[tokio::test]
async fn retry_500_then_success() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": []
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = test_client_fast_retry(&server);
    let req = SearchRequest::new("test");
    let resp = client.search().create(req).await.unwrap();

    assert!(resp.results.is_empty());
}
