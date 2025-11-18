use anthropic_async::types::{
    content::{ContentBlock, ContentBlockParam, MessageParam, MessageRole},
    messages::MessagesCreateRequest,
    tools::{Tool, ToolChoice},
};

#[test]
fn tool_serialization() {
    let tool = Tool {
        name: "get_weather".into(),
        description: Some("Get weather for a city".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "city": { "type": "string" }
            },
            "required": ["city"]
        }),
        cache_control: None,
    };
    let s = serde_json::to_string(&tool).unwrap();
    assert!(s.contains(r#""name":"get_weather""#));
    assert!(s.contains(r#""description":"Get weather for a city""#));
    assert!(s.contains(r#""input_schema""#));
}

#[test]
fn tool_choice_auto() {
    let tc = ToolChoice::Auto {
        disable_parallel_tool_use: None,
    };
    let s = serde_json::to_string(&tc).unwrap();
    assert!(s.contains(r#""type":"auto""#));
}

#[test]
fn tool_choice_specific() {
    let tc = ToolChoice::Tool {
        name: "calculator".into(),
        disable_parallel_tool_use: Some(true),
    };
    let s = serde_json::to_string(&tc).unwrap();
    assert!(s.contains(r#""type":"tool""#));
    assert!(s.contains(r#""name":"calculator""#));
    assert!(s.contains(r#""disable_parallel_tool_use":true"#));
}

#[test]
fn message_request_with_tools() {
    let tool = Tool {
        name: "echo".into(),
        description: None,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            }
        }),
        cache_control: None,
    };

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet-20241022".into(),
        max_tokens: 128,
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
        tools: Some(vec![tool]),
        tool_choice: Some(ToolChoice::Auto {
            disable_parallel_tool_use: None,
        }),
    };

    let s = serde_json::to_string(&req).unwrap();
    assert!(s.contains(r#""tools""#));
    assert!(s.contains(r#""tool_choice""#));
    assert!(s.contains(r#""name":"echo""#));
}

#[test]
fn tool_result_content_block() {
    let tool_result = ContentBlockParam::ToolResult {
        tool_use_id: "tool_123".into(),
        content: Some("Result content".into()),
        is_error: Some(false),
        cache_control: None,
    };

    let s = serde_json::to_string(&tool_result).unwrap();
    assert!(s.contains(r#""type":"tool_result""#));
    assert!(s.contains(r#""tool_use_id":"tool_123""#));
    assert!(s.contains(r#""content":"Result content""#));
    assert!(s.contains(r#""is_error":false"#));
}

#[test]
fn tool_use_response_block() {
    let tool_use = ContentBlock::ToolUse {
        id: "tool_456".into(),
        name: "calculator".into(),
        input: serde_json::json!({ "expression": "2+2" }),
    };

    let s = serde_json::to_string(&tool_use).unwrap();
    assert!(s.contains(r#""type":"tool_use""#));
    assert!(s.contains(r#""id":"tool_456""#));
    assert!(s.contains(r#""name":"calculator""#));
    assert!(s.contains(r#""expression":"2+2""#));
}

#[test]
fn tool_use_response_deserialization() {
    let json = r#"{
        "type": "tool_use",
        "id": "tool_789",
        "name": "weather",
        "input": {"city": "Paris"}
    }"#;

    let block: ContentBlock = serde_json::from_str(json).unwrap();
    match block {
        ContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, "tool_789");
            assert_eq!(name, "weather");
            assert_eq!(input["city"], "Paris");
        }
        ContentBlock::Text { .. } => panic!("Expected ToolUse variant"),
    }
}

#[cfg(feature = "schemars")]
#[test]
fn schema_generation() {
    use anthropic_async::types::tools::schema;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(tag = "action", content = "params")]
    enum TestActions {
        Add { a: i32, b: i32 },
        Multiply { a: i32, b: i32 },
    }

    let tool = schema::tool_from_schema::<TestActions>("math", Some("Math operations"));
    assert_eq!(tool.name, "math");
    assert_eq!(tool.description, Some("Math operations".into()));
    assert!(tool.input_schema.is_object());
}

#[cfg(feature = "schemars")]
#[test]
fn tool_use_parsing() {
    use anthropic_async::types::tools::schema;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
    #[serde(tag = "action", content = "params")]
    enum TestActions {
        Echo { message: String },
    }

    let input = serde_json::json!({ "message": "hello" });
    let action = schema::try_parse_tool_use::<TestActions>("Echo", &input).unwrap();

    match action {
        TestActions::Echo { message } => {
            assert_eq!(message, "hello");
        }
    }
}
