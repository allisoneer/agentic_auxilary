use anthropic_async::{AnthropicConfig, Client, types::{content::*, messages::*}};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_count_tokens() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "input_tokens": 42 })),
        )
        .mount(&server)
        .await;

    let req = MessageTokensCountRequest {
        model: "claude-3-5-haiku".into(),
        system: None,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "Count my tokens".into(),
        }],
    };

    let cfg = AnthropicConfig::new()
        .with_api_key("test")
        .with_api_base(server.uri());
    let client = Client::with_config(cfg);

    let response = client.messages().count_tokens(req).await.unwrap();
    assert_eq!(response.input_tokens, 42);
}
