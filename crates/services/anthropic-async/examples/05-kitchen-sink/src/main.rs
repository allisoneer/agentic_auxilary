//! Kitchen sink example demonstrating all features.
//!
//! This example shows:
//! - Authentication configuration
//! - Beta feature flags
//! - Prompt caching with mixed TTLs
//! - Model listing
//! - Token counting
//! - Error handling

use anthropic_async::{
    config::BetaFeature,
    types::{common::*, content::*, messages::*, ModelListParams},
    AnthropicConfig, Client,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure client with beta features
    let cfg = AnthropicConfig::new().with_beta_features([
        BetaFeature::PromptCaching20240731,
        BetaFeature::ExtendedCacheTtl20250411,
    ]);

    let client = Client::with_config(cfg);

    // List models
    println!("Available models:");
    let models = client.models().list(&ModelListParams::default()).await?;
    for model in &models.data {
        println!("  - {}", model.id);
    }

    // Count tokens first
    let system_content = SystemParam::Blocks(vec![TextBlockParam {
        text: "You are a helpful AI assistant with expertise in software development.".into(),
        kind: "text".into(),
        cache_control: Some(CacheControl::ephemeral_1h()),
    }]);

    let count_req = MessageTokensCountRequest {
        model: "claude-3-5-sonnet".into(),
        system: Some(system_content.clone()),
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "What are the benefits of Rust for systems programming?".into(),
        }],
        tools: None,
        tool_choice: None,
    };

    let token_count = client.messages().count_tokens(count_req).await?;
    println!("\nEstimated tokens: {}", token_count.input_tokens);

    // Create messages with caching
    println!("\nSending cached requests...");

    for (i, question) in [
        "What are the benefits of Rust for systems programming?",
        "How does Rust's ownership system work?",
        "What are some common Rust design patterns?",
    ]
    .iter()
    .enumerate()
    {
        let request = MessagesCreateRequest {
            model: "claude-3-5-sonnet".into(),
            max_tokens: 256,
            system: Some(system_content.clone()),
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: MessageContentParam::Blocks(vec![ContentBlockParam::Text {
                    text: (*question).to_string(),
                    cache_control: Some(CacheControl::ephemeral_5m()),
                }]),
            }],
            temperature: Some(0.3),
            stop_sequences: None,
            top_p: None,
            top_k: None,
            metadata: None,
            tools: None,
            tool_choice: None,
            stream: None,
            output_format: None,
        };

        let response = client.messages().create(request).await?;

        println!("\nQuestion {}: {question}", i + 1);
        println!("Response: ");
        for block in &response.content {
            match block {
                ContentBlock::Text { text } => println!("{text}"),
                ContentBlock::ToolUse { name, .. } => println!("[Tool: {name}]"),
            }
        }

        if let Some(usage) = &response.usage {
            println!("\nUsage:");
            println!("  Input tokens: {:?}", usage.input_tokens);
            println!("  Output tokens: {:?}", usage.output_tokens);
            println!("  Cache creation: {:?}", usage.cache_creation_input_tokens);
            println!("  Cache read: {:?}", usage.cache_read_input_tokens);
        }
    }

    Ok(())
}
