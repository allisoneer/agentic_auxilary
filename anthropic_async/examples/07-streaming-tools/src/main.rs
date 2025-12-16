//! Streaming tool calling example showing real-time tool use processing.
//!
//! This example demonstrates:
//! - Streaming responses with tool definitions
//! - Processing tool_use events incrementally
//! - Accumulating partial JSON for tool inputs
//! - Using the Accumulator helper to reconstruct complete messages

use anthropic_async::{
    streaming::{Accumulator, ContentBlockDeltaData, ContentBlockStartData, Event},
    types::{
        content::*,
        messages::*,
        tools::{schema, ToolChoice},
    },
    AnthropicConfig, Client,
};
use futures::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", content = "params", rename_all = "snake_case")]
enum MyTools {
    GetWeather { city: String },
    GetTime { timezone: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AnthropicConfig::new();
    let client = Client::with_config(cfg);

    // Generate tool schema from enum
    let tools = vec![schema::tool_from_schema::<MyTools>(
        "my_tools",
        Some("Tool dispatcher for weather and time"),
    )];

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 256,
        system: None,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "What's the weather like in Paris and what time is it in Tokyo?".into(),
        }],
        temperature: None,
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: Some(tools),
        tool_choice: Some(ToolChoice::Auto {
            disable_parallel_tool_use: None,
        }),
        stream: None,
        output_format: None,
    };

    println!("Streaming response with tool calls:\n");

    let mut stream = client.messages().create_stream(req).await?;
    let mut accumulator = Accumulator::new();

    while let Some(event_result) = stream.next().await {
        let event = event_result?;

        // Apply event to accumulator for final response reconstruction
        if let Some(response) = accumulator.apply(&event)? {
            // Message complete, print final tool calls
            println!("\n\n=== Final Response ===");
            for (i, block) in response.content.iter().enumerate() {
                match block {
                    ContentBlock::Text { text } => {
                        println!("Block {i} (text): {text}");
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        println!("Block {i} (tool_use):");
                        println!("  ID: {id}");
                        println!("  Name: {name}");
                        println!("  Input: {}", serde_json::to_string_pretty(input)?);

                        // Parse back to typed enum
                        if let Ok(action) = schema::try_parse_tool_use::<MyTools>(name, input) {
                            println!("  Parsed action: {action:?}");
                        }
                    }
                }
            }
            break;
        }

        // Also show streaming progress
        match event {
            Event::MessageStart { message } => {
                println!("[Message started: {}]", message.id);
            }
            Event::ContentBlockStart {
                index,
                content_block,
            } => match content_block {
                ContentBlockStartData::Text { .. } => {
                    print!("[Block {index}: text] ");
                }
                ContentBlockStartData::ToolUse { id, name, .. } => {
                    println!("\n[Block {index}: tool_use '{name}' (id: {id})]");
                    print!("  Input JSON: ");
                }
            },
            Event::ContentBlockDelta { delta, .. } => match delta {
                ContentBlockDeltaData::TextDelta { text } => {
                    print!("{text}");
                    io::stdout().flush()?;
                }
                ContentBlockDeltaData::InputJsonDelta { partial_json } => {
                    print!("{partial_json}");
                    io::stdout().flush()?;
                }
                _ => {}
            },
            Event::ContentBlockStop { index } => {
                println!("\n[Block {index} complete]");
            }
            Event::MessageDelta { delta, .. } => {
                if let Some(reason) = delta.stop_reason {
                    println!("[Stop reason: {reason}]");
                }
            }
            _ => {}
        }
    }

    Ok(())
}
