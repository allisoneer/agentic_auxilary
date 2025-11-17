//! Basic completion example showing simple message creation.
//!
//! This example demonstrates:
//! - Creating a client with API key
//! - Sending a basic message
//! - Receiving and displaying the response

use anthropic_client::{types::messages::*, AnthropicConfig, Client};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AnthropicConfig::new();
    let client = Client::with_config(cfg);

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 64,
        system: None,
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "Say hello in a creative way".into(),
                cache_control: None,
            }],
        }],
        temperature: Some(0.7),
    };

    println!("Sending request to Claude...");
    let res = client.messages().create(req).await?;

    println!("\nClaude's response:");
    for block in &res.content {
        match block {
            ContentBlock::Text { text, .. } => println!("{text}"),
        }
    }

    if let Some(usage) = &res.usage {
        println!("\nToken usage:");
        println!("  Input: {:?}", usage.input_tokens);
        println!("  Output: {:?}", usage.output_tokens);
    }

    Ok(())
}
