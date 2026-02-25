//! Multi-turn tool conversation conformance tests.
//!
//! These tests validate the complete multi-turn tool-calling flow:
//! 1. User sends a message that triggers tool use
//! 2. Assistant returns a `tool_use` block
//! 3. User echoes the assistant message (using `try_into_message_param()`) and sends `tool_result`
//! 4. Assistant returns a final text response
//!
//! # Running modes
//!
//! - **Replay mode** (default): Uses httpmock to replay recorded cassettes. No API key needed.
//! - **Live mode** (`ANTHROPIC_LIVE=1`): Makes real API calls. Requires `ANTHROPIC_API_KEY`.
//! - **Record mode** (`ANTHROPIC_LIVE=1 ANTHROPIC_RECORD=1`): Records API interactions to YAML.
//!
//! # Example
//!
//! ```bash
//! # Run in replay mode (CI)
//! cargo test -p anthropic-async multi_turn
//!
//! # Run in live mode (no recording)
//! ANTHROPIC_LIVE=1 cargo test -p anthropic-async multi_turn
//!
//! # Record new cassettes
//! ANTHROPIC_LIVE=1 ANTHROPIC_RECORD=1 cargo test -p anthropic-async multi_turn -- --nocapture
//! ```

mod support;

use anthropic_async::types::{
    content::{
        ContentBlock, ContentBlockParam, MessageContentParam, MessageParam, MessageRole,
        ToolResultContent,
    },
    messages::MessagesCreateRequest,
    tools::{Tool, ToolChoice},
};
use insta::assert_json_snapshot;
use support::snapshots::SnapshotHarness;

/// Creates a simple weather tool definition for testing.
fn weather_tool() -> Tool {
    Tool {
        name: "get_weather".into(),
        description: Some("Get the current weather for a location".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and country, e.g., 'Paris, France'"
                }
            },
            "required": ["location"]
        }),
        cache_control: None,
        strict: None,
    }
}

/// Extract `tool_use` ID from a response for creating `tool_result`.
fn extract_tool_use_id(content: &[ContentBlock]) -> Option<String> {
    content.iter().find_map(|block| match block {
        ContentBlock::ToolUse { id, .. } => Some(id.clone()),
        _ => None,
    })
}

/// Tests a complete multi-turn tool conversation flow.
///
/// This test validates:
/// 1. Assistant correctly returns `tool_use` when asked about weather
/// 2. We can echo the assistant's response using `try_into_message_param()`
/// 3. We can send a `tool_result` with the weather data
/// 4. Assistant responds with a final text answer
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn multi_turn_tool_conversation() {
    let harness = SnapshotHarness::new("multi_turn_tool_conversation").await;

    // Verify harness is configured correctly for the mode we're in
    if harness.is_live() {
        // Live mode: server presence depends on whether we're recording
        eprintln!("Running in LIVE mode against {}", harness.base_url());
    } else {
        // Replay mode: must have a mock server
        assert!(harness.has_server(), "Replay mode requires a mock server");
    }

    let client = harness.client();

    // --- Turn 1: User asks about weather ---
    let user_message = MessageParam {
        role: MessageRole::User,
        content: "What's the weather like in Paris right now?".into(),
    };

    let req1 = MessagesCreateRequest {
        model: "claude-sonnet-4-20250514".into(),
        max_tokens: 256,
        temperature: Some(0.0),
        messages: vec![user_message.clone()],
        tools: Some(vec![weather_tool()]),
        tool_choice: Some(ToolChoice::Tool {
            name: "get_weather".into(),
            disable_parallel_tool_use: Some(true),
        }),
        ..Default::default()
    };

    let resp1 = client
        .messages()
        .create(req1)
        .await
        .expect("First turn should succeed");

    // Verify assistant returned tool_use
    let has_tool_use = resp1
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
    assert!(
        has_tool_use,
        "Expected assistant to return tool_use block, got: {:?}",
        resp1.content
    );

    // Extract tool_use ID for the tool_result
    let tool_use_id = extract_tool_use_id(&resp1.content).expect("Should have tool_use ID");

    // Verify tool name is correct
    let tool_name = resp1.content.iter().find_map(|block| match block {
        ContentBlock::ToolUse { name, .. } => Some(name.clone()),
        _ => None,
    });
    assert_eq!(
        tool_name.as_deref(),
        Some("get_weather"),
        "Expected get_weather tool, got: {tool_name:?}"
    );

    // Snapshot turn 1 response for SDK contract verification
    assert_json_snapshot!("turn1_response", &resp1, {
        ".id" => "[redacted]",
        ".content[].id" => "[redacted]",
    });

    // --- Turn 2: Echo assistant + send tool_result ---

    // Use try_into_message_param() to echo the assistant's response
    let assistant_msg = resp1
        .try_into_message_param()
        .expect("Should be able to convert response to MessageParam");

    // Verify the echo conversion worked
    assert_eq!(assistant_msg.role, MessageRole::Assistant);
    match &assistant_msg.content {
        MessageContentParam::Blocks(blocks) => {
            let has_tool_use_param = blocks
                .iter()
                .any(|b| matches!(b, ContentBlockParam::ToolUse { .. }));
            assert!(
                has_tool_use_param,
                "Echoed message should contain ToolUse block"
            );
        }
        MessageContentParam::String(_) => panic!("Expected Blocks content after echo conversion"),
    }

    // Create tool_result with mock weather data
    let tool_result = ContentBlockParam::ToolResult {
        tool_use_id,
        content: Some(ToolResultContent::String(
            "Currently sunny with a temperature of 22°C (72°F). Light breeze from the west.".into(),
        )),
        is_error: None,
        cache_control: None,
    };

    let tool_result_message = MessageParam {
        role: MessageRole::User,
        content: MessageContentParam::Blocks(vec![tool_result]),
    };

    let req2 = MessagesCreateRequest {
        model: "claude-sonnet-4-20250514".into(),
        max_tokens: 256,
        temperature: Some(0.0),
        messages: vec![user_message, assistant_msg, tool_result_message],
        tools: Some(vec![weather_tool()]),
        tool_choice: Some(ToolChoice::None),
        ..Default::default()
    };

    let resp2 = client
        .messages()
        .create(req2)
        .await
        .expect("Second turn should succeed");

    // Snapshot turn 2 response for SDK contract verification
    assert_json_snapshot!("turn2_response", &resp2, {
        ".id" => "[redacted]",
    });

    // Verify final response contains text (not another tool_use)
    let has_text = resp2
        .content
        .iter()
        .any(|b| matches!(b, ContentBlock::Text { .. }));
    assert!(
        has_text,
        "Expected assistant to return text response, got: {:?}",
        resp2.content
    );

    // Verify the text mentions the weather information we provided
    let text_content: String = resp2
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();

    // The response should reference the weather data we provided
    assert!(
        text_content.to_lowercase().contains("paris")
            || text_content.to_lowercase().contains("sunny")
            || text_content.to_lowercase().contains("22")
            || text_content.to_lowercase().contains("72"),
        "Expected response to reference the weather data, got: {text_content}"
    );
}

/// Tests that the echo pattern preserves `tool_use` details correctly.
///
/// This is a synchronous unit test that verifies the `TryFrom` conversion
/// roundtrip without making API calls.
#[test]
fn echo_pattern_preserves_tool_use_details() {
    use anthropic_async::types::content::ContentBlockParam;

    // Create a ContentBlock::ToolUse as would be returned from API
    let tool_use = ContentBlock::ToolUse {
        id: "toolu_01XYZ".into(),
        name: "get_weather".into(),
        input: serde_json::json!({
            "location": "Paris, France"
        }),
    };

    // Convert to ContentBlockParam (echo pattern)
    let param = ContentBlockParam::try_from(&tool_use).expect("ToolUse should be convertible");

    // Verify the conversion preserved all fields
    match param {
        ContentBlockParam::ToolUse {
            id,
            name,
            input,
            cache_control,
        } => {
            assert_eq!(id, "toolu_01XYZ");
            assert_eq!(name, "get_weather");
            assert_eq!(input["location"], "Paris, France");
            assert!(
                cache_control.is_none(),
                "cache_control should be None after conversion"
            );
        }
        _ => panic!("Expected ToolUse variant"),
    }
}

/// Tests that serialization of the echo pattern produces correct JSON.
#[test]
fn echo_pattern_serialization() {
    // Create a tool_use block
    let tool_use = ContentBlock::ToolUse {
        id: "toolu_test123".into(),
        name: "calculator".into(),
        input: serde_json::json!({ "expression": "2 + 2" }),
    };

    // Convert to param
    let param = ContentBlockParam::try_from(&tool_use).unwrap();

    // Serialize
    let json = serde_json::to_string(&param).unwrap();

    // Verify the JSON structure
    assert!(
        json.contains(r#""type":"tool_use""#),
        "Should have type field"
    );
    assert!(json.contains(r#""id":"toolu_test123""#), "Should have id");
    assert!(json.contains(r#""name":"calculator""#), "Should have name");
    assert!(
        json.contains(r#""expression":"2 + 2""#),
        "Should have input"
    );
}

mod unit_tests {
    use super::*;

    #[test]
    fn weather_tool_schema_valid() {
        let tool = weather_tool();
        assert_eq!(tool.name, "get_weather");
        assert!(tool.description.is_some());
        assert!(tool.input_schema.is_object());
        assert!(tool.input_schema["properties"]["location"].is_object());
    }

    #[test]
    fn extract_tool_use_id_finds_id() {
        let content = vec![
            ContentBlock::Text {
                text: "Let me check the weather.".into(),
                citations: None,
            },
            ContentBlock::ToolUse {
                id: "toolu_abc123".into(),
                name: "get_weather".into(),
                input: serde_json::json!({}),
            },
        ];

        let id = extract_tool_use_id(&content);
        assert_eq!(id, Some("toolu_abc123".into()));
    }

    #[test]
    fn extract_tool_use_id_returns_none_when_no_tool_use() {
        let content = vec![ContentBlock::Text {
            text: "Just text.".into(),
            citations: None,
        }];

        let id = extract_tool_use_id(&content);
        assert!(id.is_none());
    }
}
