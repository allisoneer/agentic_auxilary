use anthropic_async::{AnthropicConfig, Client};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_models_list_retries_on_429_then_success() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));

    let count_clone = count.clone();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(move |_req: &wiremock::Request| {
            let i = count_clone.fetch_add(1, Ordering::SeqCst);
            if i == 0 {
                ResponseTemplate::new(429)
                    .insert_header("retry-after-ms", "100")
                    .set_body_json(
                        json!({"error": {"message": "Rate limit", "type": "rate_limit_error"}}),
                    )
            } else {
                ResponseTemplate::new(200).set_body_json(json!({"data": [], "has_more": false}))
            }
        })
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test");
    let client = Client::with_config(cfg);

    let _ = client.models().list(&()).await.unwrap();
    assert!(count.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn test_models_get_retries_on_500_then_success() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));

    let count_clone = count.clone();
    Mock::given(method("GET"))
        .and(path("/v1/models/claude-foo"))
        .respond_with(move |_req: &wiremock::Request| {
            let i = count_clone.fetch_add(1, Ordering::SeqCst);
            if i == 0 {
                ResponseTemplate::new(500).set_body_string("server error")
            } else {
                ResponseTemplate::new(200).set_body_json(json!({
                    "id": "claude-foo",
                    "created_at": "2024-06-01T12:00:00Z",
                    "type": "model"
                }))
            }
        })
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test");
    let client = Client::with_config(cfg);

    let m = client.models().get("claude-foo").await.unwrap();
    assert_eq!(m.id, "claude-foo");
}
