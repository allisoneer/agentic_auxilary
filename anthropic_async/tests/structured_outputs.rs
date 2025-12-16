//! Tests for structured outputs beta functionality

use anthropic_async::BetaFeature;
use anthropic_async::types::{
    content::{
        ContentBlockParam, ImageSource, MessageParam, MessageRole, ToolResultContent,
        ToolResultContentBlock,
    },
    messages::{MessagesCreateRequest, OutputFormat},
    tools::Tool,
};

#[test]
fn test_output_format_json_schema_serialization() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name", "age"]
    });

    let output_format = OutputFormat::JsonSchema {
        schema: schema.clone(),
    };

    let s = serde_json::to_string(&output_format).unwrap();
    assert!(s.contains(r#""type":"json_schema""#));
    assert!(s.contains(r#""schema""#));
    assert!(s.contains(r#""name""#));
    assert!(s.contains(r#""age""#));

    // Test round-trip deserialization
    let parsed: OutputFormat = serde_json::from_str(&s).unwrap();
    match parsed {
        OutputFormat::JsonSchema {
            schema: parsed_schema,
        } => {
            assert_eq!(parsed_schema, schema);
        }
    }
}

#[test]
fn test_messages_request_with_output_format() {
    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet-20241022".into(),
        max_tokens: 1024,
        system: None,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "Generate a person".into(),
        }],
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
        stream: None,
        output_format: Some(OutputFormat::JsonSchema {
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                }
            }),
        }),
    };

    let s = serde_json::to_string(&req).unwrap();
    assert!(s.contains(r#""output_format""#));
    assert!(s.contains(r#""type":"json_schema""#));
}

#[test]
fn test_messages_request_omits_none_output_format() {
    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet-20241022".into(),
        max_tokens: 1024,
        system: None,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "Hello".into(),
        }],
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
        stream: None,
        output_format: None,
    };

    let s = serde_json::to_string(&req).unwrap();
    assert!(!s.contains("output_format"));
}

#[test]
fn test_messages_request_with_stream() {
    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet-20241022".into(),
        max_tokens: 1024,
        system: None,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "Hello".into(),
        }],
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
        stream: Some(true),
        output_format: None,
    };

    let s = serde_json::to_string(&req).unwrap();
    assert!(s.contains(r#""stream":true"#));
}

#[test]
fn test_beta_header_structured_outputs_versions() {
    // Test 2025-09-17 version (Python SDK)
    let beta_py: String = BetaFeature::StructuredOutputs20250917.into();
    assert_eq!(beta_py, "structured-outputs-2025-09-17");

    // Test 2025-11-13 version (TypeScript SDK, recommended)
    let beta_ts: String = BetaFeature::StructuredOutputs20251113.into();
    assert_eq!(beta_ts, "structured-outputs-2025-11-13");

    // Test Latest alias (should resolve to 2025-11-13)
    let beta_latest: String = BetaFeature::StructuredOutputsLatest.into();
    assert_eq!(beta_latest, "structured-outputs-2025-11-13");
}

#[test]
fn test_tool_strict_serialization() {
    let tool_without_strict = Tool {
        name: "calculator".into(),
        description: Some("Math calculations".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "expression": { "type": "string" }
            }
        }),
        cache_control: None,
        strict: None,
    };

    let s = serde_json::to_string(&tool_without_strict).unwrap();
    // strict should not appear when None
    assert!(!s.contains("strict"));

    let tool_with_strict = Tool {
        name: "calculator".into(),
        description: Some("Math calculations".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "expression": { "type": "string" }
            }
        }),
        cache_control: None,
        strict: Some(true),
    };

    let s = serde_json::to_string(&tool_with_strict).unwrap();
    assert!(s.contains(r#""strict":true"#));
}

#[test]
fn test_tool_result_string_content() {
    let tool_result = ContentBlockParam::ToolResult {
        tool_use_id: "tool_123".into(),
        content: Some(ToolResultContent::String("Simple string result".into())),
        is_error: None,
        cache_control: None,
    };

    let s = serde_json::to_string(&tool_result).unwrap();
    assert!(s.contains(r#""content":"Simple string result""#));
}

#[test]
fn test_tool_result_blocks_content() {
    let tool_result = ContentBlockParam::ToolResult {
        tool_use_id: "tool_456".into(),
        content: Some(ToolResultContent::Blocks(vec![
            ToolResultContentBlock::Text {
                text: "First block".into(),
                cache_control: None,
            },
            ToolResultContentBlock::Text {
                text: "Second block".into(),
                cache_control: None,
            },
        ])),
        is_error: None,
        cache_control: None,
    };

    let s = serde_json::to_string(&tool_result).unwrap();
    assert!(s.contains(r#""type":"tool_result""#));
    assert!(s.contains(r#""First block""#));
    assert!(s.contains(r#""Second block""#));
    // Content should be an array
    assert!(s.contains(r#""content":[{"#));
}

#[test]
fn test_tool_result_content_with_image() {
    let tool_result = ContentBlockParam::ToolResult {
        tool_use_id: "tool_789".into(),
        content: Some(ToolResultContent::Blocks(vec![
            ToolResultContentBlock::Text {
                text: "Image description".into(),
                cache_control: None,
            },
            ToolResultContentBlock::Image {
                source: ImageSource::Base64 {
                    media_type: "image/png".into(),
                    data: "iVBORw0KGgo...".into(),
                },
                cache_control: None,
            },
        ])),
        is_error: None,
        cache_control: None,
    };

    let s = serde_json::to_string(&tool_result).unwrap();
    assert!(s.contains(r#""type":"text""#));
    assert!(s.contains(r#""type":"image""#));
    assert!(s.contains(r#""media_type":"image/png""#));
}

#[test]
fn test_tool_result_content_from_string() {
    // Test From<&str> conversion
    let content: ToolResultContent = "test result".into();
    match content {
        ToolResultContent::String(s) => assert_eq!(s, "test result"),
        ToolResultContent::Blocks(_) => panic!("Expected String variant"),
    }

    // Test From<String> conversion
    let content: ToolResultContent = String::from("another result").into();
    match content {
        ToolResultContent::String(s) => assert_eq!(s, "another result"),
        ToolResultContent::Blocks(_) => panic!("Expected String variant"),
    }
}

#[test]
fn test_tool_result_content_deserialization() {
    // Test string content deserialization
    let json_string = r#""simple string result""#;
    let content: ToolResultContent = serde_json::from_str(json_string).unwrap();
    match content {
        ToolResultContent::String(s) => assert_eq!(s, "simple string result"),
        ToolResultContent::Blocks(_) => panic!("Expected String variant"),
    }

    // Test blocks content deserialization
    let json_blocks = r#"[{"type":"text","text":"block text"}]"#;
    let content: ToolResultContent = serde_json::from_str(json_blocks).unwrap();
    match content {
        ToolResultContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ToolResultContentBlock::Text { text, .. } => {
                    assert_eq!(text, "block text");
                }
                ToolResultContentBlock::Image { .. } => panic!("Expected Text block"),
            }
        }
        ToolResultContent::String(_) => panic!("Expected Blocks variant"),
    }
}
