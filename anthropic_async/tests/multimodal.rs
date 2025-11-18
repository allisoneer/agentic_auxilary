use anthropic_async::{
    AnthropicConfig, Client,
    types::{
        common::CacheControl,
        content::{
            ContentBlockParam, DocumentSource, ImageSource, MessageContentParam, MessageParam,
            MessageRole,
        },
        messages::MessagesCreateRequest,
    },
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_multimodal_content_serialization() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "I can see the image and document."}],
            "model": "claude-3-5-sonnet-20241022",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 100, "output_tokens": 20}
        })))
        .mount(&server)
        .await;

    let image = ContentBlockParam::Image {
        source: ImageSource::Url {
            url: "https://example.com/image.jpg".into(),
        },
        cache_control: None,
    };

    let document = ContentBlockParam::Document {
        source: DocumentSource::Base64 {
            media_type: "application/pdf".into(),
            data: "base64data".into(),
        },
        cache_control: Some(CacheControl::ephemeral_1h()),
    };

    let text = ContentBlockParam::Text {
        text: "What's in these files?".into(),
        cache_control: None,
    };

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet-20241022".into(),
        max_tokens: 128,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: MessageContentParam::Blocks(vec![image, document, text]),
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

    let cfg = AnthropicConfig::new()
        .with_api_key("test")
        .with_api_base(server.uri());
    let client = Client::with_config(cfg);

    let response = client.messages().create(req).await.unwrap();
    assert_eq!(response.id, "msg_123");
    assert_eq!(response.content.len(), 1);
}

#[test]
fn test_image_source_url_serialization() {
    let source = ImageSource::Url {
        url: "https://example.com/test.png".into(),
    };
    let json = serde_json::to_string(&source).unwrap();
    assert!(json.contains(r#""type":"url""#));
    assert!(json.contains(r#""url":"https://example.com/test.png""#));
}

#[test]
fn test_image_source_base64_serialization() {
    let source = ImageSource::Base64 {
        media_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
    };
    let json = serde_json::to_string(&source).unwrap();
    assert!(json.contains(r#""type":"base64""#));
    assert!(json.contains(r#""media_type":"image/png""#));
    assert!(json.contains(r#""data":"iVBORw0KGgo=""#));
}

#[test]
fn test_document_source_url_serialization() {
    let source = DocumentSource::Url {
        url: "https://example.com/doc.pdf".into(),
    };
    let json = serde_json::to_string(&source).unwrap();
    assert!(json.contains(r#""type":"url""#));
    assert!(json.contains(r#""url":"https://example.com/doc.pdf""#));
}

#[test]
fn test_document_source_base64_serialization() {
    let source = DocumentSource::Base64 {
        media_type: "application/pdf".into(),
        data: "JVBERi0=".into(),
    };
    let json = serde_json::to_string(&source).unwrap();
    assert!(json.contains(r#""type":"base64""#));
    assert!(json.contains(r#""media_type":"application/pdf""#));
    assert!(json.contains(r#""data":"JVBERi0=""#));
}

#[test]
fn test_content_block_image_serialization() {
    let block = ContentBlockParam::Image {
        source: ImageSource::Url {
            url: "https://example.com/image.jpg".into(),
        },
        cache_control: Some(CacheControl::ephemeral_5m()),
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains(r#""type":"image""#));
    assert!(json.contains(r#""source""#));
    assert!(json.contains(r#""cache_control""#));
}

#[test]
fn test_content_block_document_serialization() {
    let block = ContentBlockParam::Document {
        source: DocumentSource::Base64 {
            media_type: "application/pdf".into(),
            data: "data".into(),
        },
        cache_control: None,
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains(r#""type":"document""#));
    assert!(json.contains(r#""source""#));
    assert!(!json.contains(r#""cache_control""#));
}
