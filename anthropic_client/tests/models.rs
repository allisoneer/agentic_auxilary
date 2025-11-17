use anthropic_client::{AnthropicConfig, Client};
use serde_json::json;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_models_list_headers_and_parse() {
    let server = MockServer::start().await;

    let body = json!({
        "data": [
          {
            "id": "claude-3-5-sonnet",
            "created_at": "2024-06-01T12:00:00Z",
            "display_name": "Claude 3.5 Sonnet",
            "type": "model"
          }
        ],
        "has_more": false
    });

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header_exists("x-api-key"))
        .and(header_exists("anthropic-version"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test");
    let client = Client::with_config(cfg);

    let res = client.models().list(&()).await.unwrap();
    assert_eq!(res.data.len(), 1);
    assert_eq!(res.data[0].id, "claude-3-5-sonnet");
}

#[tokio::test]
async fn test_models_get() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models/claude-3-5-sonnet"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "claude-3-5-sonnet",
            "created_at": "2024-06-01T12:00:00Z",
            "type": "model"
        })))
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test");
    let client = Client::with_config(cfg);

    let m = client.models().get("claude-3-5-sonnet").await.unwrap();
    assert_eq!(m.id, "claude-3-5-sonnet");
    assert_eq!(m.kind, "model");
}
