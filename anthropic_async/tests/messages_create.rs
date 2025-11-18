use anthropic_async::{
    AnthropicConfig, Client,
    types::{common::*, content::*, messages::*},
};
use serde_json::json;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_messages_create_with_caching() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header_exists("x-api-key"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "text",
                "text": "Hello!"
            }],
            "model": "claude-3-5-sonnet",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 10,
                "cache_creation_input_tokens": 80,
                "cache_read_input_tokens": 0
            }
        })))
        .mount(&server)
        .await;

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 64,
        system: Some(SystemParam::Blocks(vec![TextBlockParam::with_cache_control(
            "You are helpful",
            CacheControl::ephemeral_1h(),
        )])),
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: MessageContentParam::Blocks(vec![ContentBlockParam::Text {
                text: "Hello".into(),
                cache_control: Some(CacheControl::ephemeral_5m()),
            }]),
        }],
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
    };

    let cfg = AnthropicConfig::new()
        .with_api_key("test-key")
        .with_api_base(server.uri());
    let client = Client::with_config(cfg);

    let response = client.messages().create(req).await.unwrap();
    assert_eq!(response.kind, "message");
    assert_eq!(
        response.usage.unwrap().cache_creation_input_tokens.unwrap(),
        80
    );
}

#[tokio::test]
async fn test_ttl_ordering_validation() {
    let server = MockServer::start().await;

    // This request should fail client-side validation
    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 64,
        system: Some(SystemParam::Blocks(vec![TextBlockParam::with_cache_control(
            "System",
            CacheControl::ephemeral_5m(), // 5m first
        )])),
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: MessageContentParam::Blocks(vec![ContentBlockParam::Text {
                text: "User".into(),
                cache_control: Some(CacheControl::ephemeral_1h()), // 1h after 5m = invalid
            }]),
        }],
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
    };

    let cfg = AnthropicConfig::new()
        .with_api_key("test-key")
        .with_api_base(server.uri());
    let client = Client::with_config(cfg);

    let err = client.messages().create(req).await.unwrap_err();
    match err {
        anthropic_async::AnthropicError::Config(msg) => {
            assert!(msg.contains("TTL ordering"));
        }
        _ => panic!("Expected Config error"),
    }
}
