//! Streaming tests for SSE parsing and event deserialization
//!
//! These tests verify the SSE decoder, event mapping, and accumulator functionality.

#![cfg(feature = "streaming")]

use anthropic_async::streaming::{
    Accumulator, ContentBlockDeltaData, ContentBlockStartData, Event, MessageDeltaPayload,
    MessageDeltaUsage, MessageStartPayload, MessageStartUsage, SSEDecoder, SseFrame,
};
use anthropic_async::types::content::{ContentBlock, MessageRole};

// =============================================================================
// SSE Decoder Tests
// =============================================================================

#[test]
fn sse_decoder_single_event() {
    let mut decoder = SSEDecoder::new();
    let chunk = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n";
    let frames = decoder.push(chunk);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].event, Some("message_start".to_string()));
    assert!(frames[0].data.contains("message_start"));
}

#[test]
fn sse_decoder_multiline_data() {
    let mut decoder = SSEDecoder::new();
    let chunk = b"event: test\ndata: line1\ndata: line2\ndata: line3\n\n";
    let frames = decoder.push(chunk);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].data, "line1\nline2\nline3");
}

#[test]
fn sse_decoder_split_chunks() {
    let mut decoder = SSEDecoder::new();

    // First chunk - incomplete
    let frames1 = decoder.push(b"event: test\nda");
    assert!(frames1.is_empty());

    // Second chunk - completes the frame
    let frames2 = decoder.push(b"ta: hello\n\n");
    assert_eq!(frames2.len(), 1);
    assert_eq!(frames2[0].event, Some("test".to_string()));
    assert_eq!(frames2[0].data, "hello");
}

#[test]
fn sse_decoder_multiple_events_single_chunk() {
    let mut decoder = SSEDecoder::new();
    let chunk = b"event: first\ndata: one\n\nevent: second\ndata: two\n\n";
    let frames = decoder.push(chunk);
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].event, Some("first".to_string()));
    assert_eq!(frames[0].data, "one");
    assert_eq!(frames[1].event, Some("second".to_string()));
    assert_eq!(frames[1].data, "two");
}

#[test]
fn sse_decoder_empty_data_line() {
    let mut decoder = SSEDecoder::new();
    let chunk = b"event: test\ndata: \n\n";
    let frames = decoder.push(chunk);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].data, "");
}

#[test]
fn sse_decoder_flush_incomplete() {
    let mut decoder = SSEDecoder::new();
    decoder.push(b"event: test\ndata: incomplete");

    // No frame yet (no blank line terminator)
    let frame = decoder.flush();
    assert!(frame.is_some());
    let f = frame.unwrap();
    assert_eq!(f.event, Some("test".to_string()));
    assert_eq!(f.data, "incomplete");
}

// =============================================================================
// Event Mapping Tests
// =============================================================================

#[test]
fn event_mapping_message_start() {
    let frame = SseFrame {
        event: Some("message_start".to_string()),
        data: r#"{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","model":"claude-3-5-sonnet","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}"#.to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    match event {
        Event::MessageStart { message } => {
            assert_eq!(message.id, "msg_123");
            assert_eq!(message.model, "claude-3-5-sonnet");
            assert_eq!(message.role, MessageRole::Assistant);
        }
        _ => panic!("Expected MessageStart"),
    }
}

#[test]
fn event_mapping_content_block_start_text() {
    let frame = SseFrame {
        event: Some("content_block_start".to_string()),
        data:
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#
                .to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    match event {
        Event::ContentBlockStart {
            index,
            content_block,
        } => {
            assert_eq!(index, 0);
            match content_block {
                ContentBlockStartData::Text { text } => {
                    assert_eq!(text, "");
                }
                ContentBlockStartData::ToolUse { .. } => panic!("Expected Text content block"),
            }
        }
        _ => panic!("Expected ContentBlockStart"),
    }
}

#[test]
fn event_mapping_content_block_start_tool_use() {
    let frame = SseFrame {
        event: Some("content_block_start".to_string()),
        data: r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool_123","name":"get_weather","input":{}}}"#.to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    match event {
        Event::ContentBlockStart {
            index,
            content_block,
        } => {
            assert_eq!(index, 0);
            match content_block {
                ContentBlockStartData::ToolUse { id, name, .. } => {
                    assert_eq!(id, "tool_123");
                    assert_eq!(name, "get_weather");
                }
                ContentBlockStartData::Text { .. } => panic!("Expected ToolUse content block"),
            }
        }
        _ => panic!("Expected ContentBlockStart"),
    }
}

#[test]
fn event_mapping_content_block_delta_text() {
    let frame = SseFrame {
        event: Some("content_block_delta".to_string()),
        data: r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#
            .to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    match event {
        Event::ContentBlockDelta { index, delta } => {
            assert_eq!(index, 0);
            match delta {
                ContentBlockDeltaData::TextDelta { text } => {
                    assert_eq!(text, "Hello");
                }
                _ => panic!("Expected TextDelta"),
            }
        }
        _ => panic!("Expected ContentBlockDelta"),
    }
}

#[test]
fn event_mapping_content_block_delta_input_json() {
    let frame = SseFrame {
        event: Some("content_block_delta".to_string()),
        data: r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"city\":"}}"#.to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    match event {
        Event::ContentBlockDelta { index, delta } => {
            assert_eq!(index, 0);
            match delta {
                ContentBlockDeltaData::InputJsonDelta { partial_json } => {
                    assert_eq!(partial_json, "{\"city\":");
                }
                _ => panic!("Expected InputJsonDelta"),
            }
        }
        _ => panic!("Expected ContentBlockDelta"),
    }
}

#[test]
fn event_mapping_message_delta() {
    let frame = SseFrame {
        event: Some("message_delta".to_string()),
        data: r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}"#.to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    match event {
        Event::MessageDelta { delta, usage } => {
            assert_eq!(delta.stop_reason, Some("end_turn".to_string()));
            assert_eq!(usage.unwrap().output_tokens, 15);
        }
        _ => panic!("Expected MessageDelta"),
    }
}

#[test]
fn event_mapping_message_stop() {
    let frame = SseFrame {
        event: Some("message_stop".to_string()),
        data: r#"{"type":"message_stop"}"#.to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    assert!(matches!(event, Event::MessageStop));
}

#[test]
fn event_mapping_ping() {
    let frame = SseFrame {
        event: Some("ping".to_string()),
        data: r#"{"type":"ping"}"#.to_string(),
    };
    let event = Event::from_frame(&frame).unwrap();
    assert!(matches!(event, Event::Ping));
}

#[test]
fn event_mapping_unknown_event_type() {
    let frame = SseFrame {
        event: Some("future_event_type".to_string()),
        data: r#"{"type":"future_event_type"}"#.to_string(),
    };
    let result = Event::from_frame(&frame);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unknown event type")
    );
}

// =============================================================================
// Accumulator Tests
// =============================================================================

#[test]
fn accumulator_text_blocks() {
    let mut acc = Accumulator::new();

    // message_start
    acc.apply(&Event::MessageStart {
        message: MessageStartPayload {
            id: "msg_test".to_string(),
            kind: "message".to_string(),
            role: MessageRole::Assistant,
            model: "claude".to_string(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: Some(MessageStartUsage {
                input_tokens: 10,
                output_tokens: 0,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            }),
        },
    })
    .unwrap();

    // content_block_start
    acc.apply(&Event::ContentBlockStart {
        index: 0,
        content_block: ContentBlockStartData::Text {
            text: String::new(),
        },
    })
    .unwrap();

    // content_block_delta (multiple)
    acc.apply(&Event::ContentBlockDelta {
        index: 0,
        delta: ContentBlockDeltaData::TextDelta {
            text: "Hello, ".to_string(),
        },
    })
    .unwrap();

    acc.apply(&Event::ContentBlockDelta {
        index: 0,
        delta: ContentBlockDeltaData::TextDelta {
            text: "world!".to_string(),
        },
    })
    .unwrap();

    assert_eq!(acc.current_text(), "Hello, world!");

    // content_block_stop
    acc.apply(&Event::ContentBlockStop { index: 0 }).unwrap();

    // message_delta
    acc.apply(&Event::MessageDelta {
        delta: MessageDeltaPayload {
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
        },
        usage: Some(MessageDeltaUsage { output_tokens: 3 }),
    })
    .unwrap();

    // message_stop
    let response = acc.apply(&Event::MessageStop).unwrap().unwrap();

    assert_eq!(response.id, "msg_test");
    assert_eq!(response.content.len(), 1);
    match &response.content[0] {
        ContentBlock::Text { text } => assert_eq!(text, "Hello, world!"),
        ContentBlock::ToolUse { .. } => panic!("Expected Text block"),
    }
    assert_eq!(response.stop_reason, Some("end_turn".to_string()));
}

#[test]
fn accumulator_tool_use_input_json() {
    let mut acc = Accumulator::new();

    // message_start
    acc.apply(&Event::MessageStart {
        message: MessageStartPayload {
            id: "msg_tool".to_string(),
            kind: "message".to_string(),
            role: MessageRole::Assistant,
            model: "claude".to_string(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: None,
        },
    })
    .unwrap();

    // content_block_start (tool_use)
    acc.apply(&Event::ContentBlockStart {
        index: 0,
        content_block: ContentBlockStartData::ToolUse {
            id: "tool_123".to_string(),
            name: "get_weather".to_string(),
            input: serde_json::json!({}),
        },
    })
    .unwrap();

    // input_json_delta (multiple)
    acc.apply(&Event::ContentBlockDelta {
        index: 0,
        delta: ContentBlockDeltaData::InputJsonDelta {
            partial_json: r#"{"city":"#.to_string(),
        },
    })
    .unwrap();

    acc.apply(&Event::ContentBlockDelta {
        index: 0,
        delta: ContentBlockDeltaData::InputJsonDelta {
            partial_json: r#""Paris"}"#.to_string(),
        },
    })
    .unwrap();

    // content_block_stop
    acc.apply(&Event::ContentBlockStop { index: 0 }).unwrap();

    // message_delta
    acc.apply(&Event::MessageDelta {
        delta: MessageDeltaPayload {
            stop_reason: Some("tool_use".to_string()),
            stop_sequence: None,
        },
        usage: None,
    })
    .unwrap();

    // message_stop
    let response = acc.apply(&Event::MessageStop).unwrap().unwrap();

    assert_eq!(response.content.len(), 1);
    match &response.content[0] {
        ContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, "tool_123");
            assert_eq!(name, "get_weather");
            assert_eq!(input["city"], "Paris");
        }
        ContentBlock::Text { .. } => panic!("Expected ToolUse block"),
    }
}

#[test]
fn accumulator_tool_use_json_parse_error() {
    let mut acc = Accumulator::new();

    // message_start
    acc.apply(&Event::MessageStart {
        message: MessageStartPayload {
            id: "msg_tool".to_string(),
            kind: "message".to_string(),
            role: MessageRole::Assistant,
            model: "claude".to_string(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: None,
        },
    })
    .unwrap();

    // content_block_start (tool_use)
    acc.apply(&Event::ContentBlockStart {
        index: 0,
        content_block: ContentBlockStartData::ToolUse {
            id: "tool_123".to_string(),
            name: "get_weather".to_string(),
            input: serde_json::json!({}),
        },
    })
    .unwrap();

    // Invalid JSON delta
    acc.apply(&Event::ContentBlockDelta {
        index: 0,
        delta: ContentBlockDeltaData::InputJsonDelta {
            partial_json: r#"{"city": invalid"#.to_string(),
        },
    })
    .unwrap();

    // content_block_stop
    acc.apply(&Event::ContentBlockStop { index: 0 }).unwrap();

    // message_delta
    acc.apply(&Event::MessageDelta {
        delta: MessageDeltaPayload {
            stop_reason: Some("tool_use".to_string()),
            stop_sequence: None,
        },
        usage: None,
    })
    .unwrap();

    // message_stop - should fail due to invalid JSON
    let result = acc.apply(&Event::MessageStop);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("tool input JSON"));
}

#[test]
fn accumulator_handles_ping_events() {
    let mut acc = Accumulator::new();

    // message_start
    acc.apply(&Event::MessageStart {
        message: MessageStartPayload {
            id: "msg_test".to_string(),
            kind: "message".to_string(),
            role: MessageRole::Assistant,
            model: "claude".to_string(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: None,
        },
    })
    .unwrap();

    // Ping should be ignored
    let result = acc.apply(&Event::Ping).unwrap();
    assert!(result.is_none());
}

#[test]
fn accumulator_handles_unknown_delta_types() {
    let mut acc = Accumulator::new();

    // message_start
    acc.apply(&Event::MessageStart {
        message: MessageStartPayload {
            id: "msg_test".to_string(),
            kind: "message".to_string(),
            role: MessageRole::Assistant,
            model: "claude".to_string(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: None,
        },
    })
    .unwrap();

    // content_block_start
    acc.apply(&Event::ContentBlockStart {
        index: 0,
        content_block: ContentBlockStartData::Text {
            text: String::new(),
        },
    })
    .unwrap();

    // Unknown delta type should be ignored
    let result = acc
        .apply(&Event::ContentBlockDelta {
            index: 0,
            delta: ContentBlockDeltaData::Unknown,
        })
        .unwrap();
    assert!(result.is_none());
}

#[test]
fn accumulator_delta_invalid_index() {
    let mut acc = Accumulator::new();

    // message_start
    acc.apply(&Event::MessageStart {
        message: MessageStartPayload {
            id: "msg_test".to_string(),
            kind: "message".to_string(),
            role: MessageRole::Assistant,
            model: "claude".to_string(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: None,
        },
    })
    .unwrap();

    // Delta for non-existent block index should error
    let result = acc.apply(&Event::ContentBlockDelta {
        index: 5,
        delta: ContentBlockDeltaData::TextDelta {
            text: "test".to_string(),
        },
    });
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("unknown block index")
    );
}
