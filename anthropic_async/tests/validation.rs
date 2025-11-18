use anthropic_async::{AnthropicConfig, Client, types::{content::*, messages::*}};
use wiremock::{MockServer, ResponseTemplate, Mock};
use wiremock::matchers::{method, path};

#[tokio::test]
async fn test_temperature_validation_below_range() {
    let server = MockServer::start().await;
    let client = Client::with_config(
        AnthropicConfig::new()
            .with_api_key("test")
            .with_api_base(server.uri())
    );

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 100,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }],
        system: None,
        temperature: Some(-0.1), // Invalid: below 0.0
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let err = client.messages().create(req).await.unwrap_err();
    match err {
        anthropic_async::AnthropicError::Config(msg) => {
            assert!(msg.contains("temperature"));
            assert!(msg.contains("-0.1"));
        }
        _ => panic!("Expected Config error"),
    }
}

#[tokio::test]
async fn test_temperature_validation_above_range() {
    let server = MockServer::start().await;
    let client = Client::with_config(
        AnthropicConfig::new()
            .with_api_key("test")
            .with_api_base(server.uri())
    );

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 100,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }],
        system: None,
        temperature: Some(1.1), // Invalid: above 1.0
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let err = client.messages().create(req).await.unwrap_err();
    match err {
        anthropic_async::AnthropicError::Config(msg) => {
            assert!(msg.contains("temperature"));
            assert!(msg.contains("1.1"));
        }
        _ => panic!("Expected Config error"),
    }
}

#[tokio::test]
async fn test_top_p_validation_zero() {
    let server = MockServer::start().await;
    let client = Client::with_config(
        AnthropicConfig::new()
            .with_api_key("test")
            .with_api_base(server.uri())
    );

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 100,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }],
        system: None,
        temperature: None,
        stop_sequences: None,
        top_p: Some(0.0), // Invalid: must be > 0.0
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let err = client.messages().create(req).await.unwrap_err();
    match err {
        anthropic_async::AnthropicError::Config(msg) => {
            assert!(msg.contains("top_p"));
        }
        _ => panic!("Expected Config error"),
    }
}

#[tokio::test]
async fn test_top_p_validation_above_range() {
    let server = MockServer::start().await;
    let client = Client::with_config(
        AnthropicConfig::new()
            .with_api_key("test")
            .with_api_base(server.uri())
    );

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 100,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }],
        system: None,
        temperature: None,
        stop_sequences: None,
        top_p: Some(1.5), // Invalid: above 1.0
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let err = client.messages().create(req).await.unwrap_err();
    match err {
        anthropic_async::AnthropicError::Config(msg) => {
            assert!(msg.contains("top_p"));
            assert!(msg.contains("1.5"));
        }
        _ => panic!("Expected Config error"),
    }
}

#[tokio::test]
async fn test_top_k_validation_zero() {
    let server = MockServer::start().await;
    let client = Client::with_config(
        AnthropicConfig::new()
            .with_api_key("test")
            .with_api_base(server.uri())
    );

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 100,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }],
        system: None,
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: Some(0), // Invalid: must be >= 1
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let err = client.messages().create(req).await.unwrap_err();
    match err {
        anthropic_async::AnthropicError::Config(msg) => {
            assert!(msg.contains("top_k"));
            assert!(msg.contains("0"));
        }
        _ => panic!("Expected Config error"),
    }
}

#[tokio::test]
async fn test_max_tokens_validation_zero() {
    let server = MockServer::start().await;
    let client = Client::with_config(
        AnthropicConfig::new()
            .with_api_key("test")
            .with_api_base(server.uri())
    );

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 0, // Invalid: must be > 0
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }],
        system: None,
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let err = client.messages().create(req).await.unwrap_err();
    match err {
        anthropic_async::AnthropicError::Config(msg) => {
            assert!(msg.contains("max_tokens"));
        }
        _ => panic!("Expected Config error"),
    }
}

#[tokio::test]
async fn test_builder_pattern_basic() {
    let req = MessagesCreateRequestBuilder::default()
        .model("claude-3-5-sonnet")
        .max_tokens(100_u32)
        .messages(vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }])
        .build()
        .unwrap();

    assert_eq!(req.model, "claude-3-5-sonnet");
    assert_eq!(req.max_tokens, 100);
    assert_eq!(req.messages.len(), 1);
}

#[tokio::test]
async fn test_builder_pattern_with_optional_params() {
    let req = MessagesCreateRequestBuilder::default()
        .model("claude-3-5-sonnet")
        .max_tokens(100_u32)
        .messages(vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }])
        .temperature(0.7)
        .top_k(5_u32)
        .stop_sequences(vec!["STOP".to_string()])
        .build()
        .unwrap();

    assert_eq!(req.temperature, Some(0.7));
    assert_eq!(req.top_k, Some(5));
    assert_eq!(req.stop_sequences, Some(vec!["STOP".to_string()]));
}

#[tokio::test]
async fn test_valid_parameters_accepted() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Response"}],
            "model": "claude-3-5-sonnet",
        })))
        .mount(&server)
        .await;

    let client = Client::with_config(
        AnthropicConfig::new()
            .with_api_key("test")
            .with_api_base(server.uri())
    );

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 100,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "test".into(),
        }],
        system: None,
        temperature: Some(0.5), // Valid
        stop_sequences: Some(vec!["END".into()]),
        top_p: Some(0.9), // Valid
        top_k: Some(10), // Valid
        metadata: None,
        tools: None,
        tool_choice: None,
    };

    let response = client.messages().create(req).await.unwrap();
    assert_eq!(response.id, "msg_123");
}
