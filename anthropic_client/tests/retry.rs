use anthropic_client::{AnthropicConfig, Client, types::messages::*};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_retry_on_429_then_success() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));

    let count_clone = count.clone();
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(move |_req: &wiremock::Request| {
            let i = count_clone.fetch_add(1, Ordering::SeqCst);
            if i == 0 {
                ResponseTemplate::new(429)
                    .insert_header("retry-after-ms", "100")
                    .set_body_json(serde_json::json!({
                        "error": {
                            "message": "Rate limit exceeded",
                            "type": "rate_limit_error"
                        }
                    }))
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "msg",
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": "Success"}],
                    "model": "claude"
                }))
            }
        })
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test");
    let client = Client::with_config(cfg);

    let request = MessagesCreateRequest {
        model: "claude".into(),
        max_tokens: 10,
        system: None,
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "test".into(),
                cache_control: None,
            }],
        }],
        temperature: None,
    };

    let response = client.messages().create(request).await.unwrap();
    assert_eq!(response.kind, "message");
    assert!(count.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn test_529_overloaded_retry() {
    let server = MockServer::start().await;
    let count = Arc::new(AtomicUsize::new(0));

    let count_clone = count.clone();
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(move |_req: &wiremock::Request| {
            let i = count_clone.fetch_add(1, Ordering::SeqCst);
            if i == 0 {
                ResponseTemplate::new(529).set_body_json(serde_json::json!({
                    "error": {
                        "message": "Overloaded",
                        "type": "overloaded_error"
                    }
                }))
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "msg",
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": "Success"}],
                    "model": "claude"
                }))
            }
        })
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test");
    let client = Client::with_config(cfg);

    let request = MessagesCreateRequest {
        model: "claude".into(),
        max_tokens: 10,
        system: None,
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "test".into(),
                cache_control: None,
            }],
        }],
        temperature: None,
    };

    let response = client.messages().create(request).await.unwrap();
    assert_eq!(response.kind, "message");
}

#[tokio::test]
async fn test_non_retryable_400() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {
                "message": "Invalid request",
                "type": "invalid_request_error"
            }
        })))
        .mount(&server)
        .await;

    let cfg = AnthropicConfig::new()
        .with_api_base(server.uri())
        .with_api_key("test");
    let client = Client::with_config(cfg);

    let request = MessagesCreateRequest {
        model: "invalid-model".into(),
        max_tokens: 10,
        system: None,
        messages: vec![],
        temperature: None,
    };

    let err = client.messages().create(request).await.unwrap_err();
    match err {
        anthropic_client::AnthropicError::Api(obj) => {
            assert_eq!(obj.message, "Invalid request");
        }
        _ => panic!("Expected Api error"),
    }
}
