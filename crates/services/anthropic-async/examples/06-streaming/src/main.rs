//! Streaming example showing real-time response processing.
//!
//! This example demonstrates:
//! - Creating a streaming message request
//! - Processing SSE events as they arrive
//! - Reconstructing text incrementally
//! - Handling different event types

use anthropic_async::{
    streaming::{ContentBlockDeltaData, Event},
    types::{content::*, messages::*},
    AnthropicConfig, Client,
};
use futures::StreamExt;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AnthropicConfig::new();
    let client = Client::with_config(cfg);

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 256,
        system: None,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "Tell me a short story about a robot learning to paint. Make it about 3 paragraphs."
                .into(),
        }],
        temperature: Some(0.8),
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
        stream: None, // Will be set to true automatically by create_stream()
        output_format: None,
    };

    println!("Streaming response from Claude:\n");

    let mut stream = client.messages().create_stream(req).await?;
    let mut total_output_tokens = 0;

    while let Some(event_result) = stream.next().await {
        match event_result? {
            Event::MessageStart { message } => {
                println!("[Message started: {}]\n", message.id);
            }
            Event::ContentBlockStart { index, content_block } => {
                match content_block {
                    anthropic_async::streaming::ContentBlockStartData::Text { .. } => {
                        print!("[Block {index} started] ");
                    }
                    anthropic_async::streaming::ContentBlockStartData::ToolUse { name, .. } => {
                        println!("[Tool use started: {name}]");
                    }
                }
            }
            Event::ContentBlockDelta { delta, .. } => {
                match delta {
                    ContentBlockDeltaData::TextDelta { text } => {
                        // Print text incrementally as it arrives
                        print!("{text}");
                        io::stdout().flush()?;
                    }
                    ContentBlockDeltaData::InputJsonDelta { partial_json } => {
                        print!("{partial_json}");
                        io::stdout().flush()?;
                    }
                    _ => {}
                }
            }
            Event::ContentBlockStop { index } => {
                println!("\n[Block {index} completed]");
            }
            Event::MessageDelta { usage, delta } => {
                if let Some(usage) = usage {
                    total_output_tokens = usage.output_tokens;
                }
                if let Some(stop_reason) = delta.stop_reason {
                    println!("\n[Stop reason: {stop_reason}]");
                }
            }
            Event::MessageStop => {
                println!("\n[Message complete]");
                break;
            }
            Event::Ping => {
                // Keep-alive, ignore
            }
            Event::Error { error } => {
                eprintln!("\n[Error: {} - {}]", error.kind, error.message);
                break;
            }
        }
    }

    println!("\nTotal output tokens: {total_output_tokens}");

    Ok(())
}
