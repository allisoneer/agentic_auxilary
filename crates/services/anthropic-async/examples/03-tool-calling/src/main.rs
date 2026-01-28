//! Tool calling example demonstrating type-safe tool usage with schemars.
//!
//! This example shows:
//! - Defining tools as a typed enum
//! - Generating tool schemas automatically
//! - Parsing tool use responses back to typed enum
//! - Handling tool results

use anthropic_async::{
    types::{
        content::*, messages::*,
        tools::{schema, ToolChoice},
    },
    AnthropicConfig, Client,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", content = "params", rename_all = "snake_case")]
enum MyTools {
    GetWeather { city: String },
    GetTime { timezone: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::with_config(AnthropicConfig::new());

    // Generate tool schema from enum
    let tools = vec![schema::tool_from_schema::<MyTools>(
        "my_tools",
        Some("Tool dispatcher for weather and time"),
    )];

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 128,
        system: None,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "What's the weather in Paris?".into(),
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

    println!("Sending request with tool definitions...");
    let response = client.messages().create(req).await?;

    println!("\nClaude's response:");
    for block in response.content {
        match block {
            ContentBlock::Text { text } => {
                println!("Text: {text}");
            }
            ContentBlock::ToolUse { id, name, input } => {
                println!("Tool use: {name} (id: {id})");
                println!("Input: {}", serde_json::to_string_pretty(&input)?);

                // Parse tool use back to typed enum
                if let Ok(action) = schema::try_parse_tool_use::<MyTools>(&name, &input) {
                    match action {
                        MyTools::GetWeather { city } => {
                            println!("→ Getting weather for {city}");
                            // Here you would call actual weather API
                        }
                        MyTools::GetTime { timezone } => {
                            println!("→ Getting time for {timezone}");
                            // Here you would get actual time
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
