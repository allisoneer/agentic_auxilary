use exa_async::test_support::EnvGuard;
use exa_async::types::search::SearchRequest;
use exa_async::{Client, ExaConfig, ExaError};
use serial_test::serial;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_client(server: &MockServer) -> Client<ExaConfig> {
    let config = ExaConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test-api-key");
    Client::with_config(config)
}

fn mock_search_response() -> serde_json::Value {
    serde_json::json!({
        "results": [
            {
                "url": "https://example.com/page1",
                "id": "abc123",
                "title": "Example Page",
                "score": 0.95,
                "publishedDate": "2025-01-15",
                "author": "Test Author",
                "text": "This is the full text content.",
                "summary": "A short summary.",
                "highlights": ["key highlight one", "key highlight two"],
                "highlightScores": [0.9, 0.8]
            }
        ],
        "autopromptString": "Here is some context about the query.",
        "costDollars": {
            "total": 0.005,
            "search": { "neural": 0.003 },
            "contents": { "text": 0.001, "highlights": 0.0005, "summary": 0.0005 }
        },
        "resolvedSearchType": "neural"
    })
}

#[tokio::test]
async fn search_success_parses() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("x-api-key", "test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_search_response()))
        .mount(&server)
        .await;

    let client = test_client(&server);
    let req = SearchRequest::new("test query").with_num_results(10);
    let resp = client.search().create(req).await.unwrap();

    assert_eq!(resp.results.len(), 1);
    assert_eq!(resp.results[0].url, "https://example.com/page1");
    assert_eq!(resp.results[0].title.as_deref(), Some("Example Page"));
    assert!((resp.results[0].score.unwrap() - 0.95).abs() < f64::EPSILON);
    assert_eq!(resp.results[0].author.as_deref(), Some("Test Author"));
    assert_eq!(
        resp.results[0].text.as_deref(),
        Some("This is the full text content.")
    );
    assert_eq!(resp.results[0].summary.as_deref(), Some("A short summary."));
    assert_eq!(
        resp.results[0].highlights.as_ref().unwrap(),
        &["key highlight one", "key highlight two"]
    );
    assert_eq!(
        resp.autoprompt_string.as_deref(),
        Some("Here is some context about the query.")
    );
    assert_eq!(resp.resolved_search_type.as_deref(), Some("neural"));

    let cost = resp.cost_dollars.as_ref().unwrap();
    assert!((cost.total.unwrap() - 0.005).abs() < 1e-12);

    let search_cost = cost.search.as_ref().unwrap();
    assert!((search_cost.neural.unwrap() - 0.003).abs() < 1e-12);
    assert!(search_cost.keyword.is_none());

    let contents_cost = cost.contents.as_ref().unwrap();
    assert!((contents_cost.text.unwrap() - 0.001).abs() < 1e-12);
    assert!((contents_cost.highlights.unwrap() - 0.0005).abs() < 1e-12);
    assert!((contents_cost.summary.unwrap() - 0.0005).abs() < 1e-12);
}

#[tokio::test]
async fn search_request_serializes_camel_case() {
    use exa_async::types::common::SearchType;

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [],
            "autopromptString": null,
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = test_client(&server);
    let req = SearchRequest::new("test")
        .with_num_results(5)
        .with_search_type(SearchType::Neural);

    let _resp = client.search().create(req).await.unwrap();

    // Verify the request was made (mock expectation will fail if not matched)
    // Additionally verify request body serialization
    let serialized = serde_json::to_value(
        SearchRequest::new("test")
            .with_num_results(5)
            .with_search_type(SearchType::Neural),
    )
    .unwrap();

    assert_eq!(serialized["query"], "test");
    assert_eq!(serialized["numResults"], 5);
    assert_eq!(serialized["type"], "neural");
}

#[tokio::test]
#[serial(env)]
async fn missing_api_key_is_config_error() {
    // Force EXA_API_KEY to be unset for deterministic test behavior
    let _guard = EnvGuard::remove("EXA_API_KEY");

    // Build a client without a key - now guaranteed to have no API key
    let client = Client::with_config(ExaConfig::new().with_api_base("http://localhost:1234"));

    let req = SearchRequest::new("test");
    let result = client.search().create(req).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ExaError::Config(msg) => assert!(msg.contains("EXA_API_KEY")),
        other => panic!("Expected Config error, got {other:?}"),
    }
}

#[tokio::test]
async fn error_429_is_retryable() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "statusCode": 429,
            "message": "Rate limit exceeded",
            "error": "Too Many Requests"
        })))
        .expect(1..)
        .mount(&server)
        .await;

    let config = ExaConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test-key");

    // Use minimal retries to keep test fast
    let client = Client::with_config(config).with_backoff(
        backon::ExponentialBuilder::default()
            .with_min_delay(std::time::Duration::from_millis(10))
            .with_max_delay(std::time::Duration::from_millis(50))
            .with_max_times(2),
    );

    let req = SearchRequest::new("test");
    let result = client.search().create(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    match &err {
        ExaError::Api(obj) => {
            assert_eq!(obj.status_code, Some(429));
            assert!(err.is_retryable());
        }
        other => panic!("Expected Api error, got {other:?}"),
    }
}

#[tokio::test]
async fn error_500_plain_text_parsed() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .expect(1..)
        .mount(&server)
        .await;

    let config = ExaConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test-key");

    let client = Client::with_config(config).with_backoff(
        backon::ExponentialBuilder::default()
            .with_min_delay(std::time::Duration::from_millis(10))
            .with_max_delay(std::time::Duration::from_millis(50))
            .with_max_times(1),
    );

    let req = SearchRequest::new("test");
    let result = client.search().create(req).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ExaError::Api(obj) => {
            assert_eq!(obj.status_code, Some(500));
            assert_eq!(obj.message, "Internal Server Error");
        }
        other => panic!("Expected Api error, got {other:?}"),
    }
}

#[tokio::test]
async fn header_auth_present() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("x-api-key", "secret-key-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": []
        })))
        .expect(1)
        .mount(&server)
        .await;

    let config = ExaConfig::new()
        .with_api_base(server.uri())
        .with_api_key("secret-key-123");

    let client = Client::with_config(config);
    let req = SearchRequest::new("test");
    let resp = client.search().create(req).await.unwrap();

    assert!(resp.results.is_empty());
    // Mock expectation with header matcher verifies the header was present
}
