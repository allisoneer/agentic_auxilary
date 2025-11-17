//! Kitchen sink example demonstrating all features.
//!
//! This example shows:
//! - Authentication configuration
//! - Beta feature flags
//! - Prompt caching with mixed TTLs
//! - Model listing
//! - Token counting
//! - Error handling

use anthropic_client::{
    config::BetaFeature,
    types::{common::*, messages::*},
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
    let models = client.models().list(&()).await?;
    for model in &models.data {
        println!("  - {}", model.id);
    }

    // Count tokens first
    let system_content = vec![ContentBlock::Text {
        text: "You are a helpful AI assistant with expertise in software development.".into(),
        cache_control: Some(CacheControl::ephemeral_1h()),
    }];

    let count_req = MessageTokensCountRequest {
        model: "claude-3-5-sonnet".into(),
        system: Some(system_content.clone()),
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "What are the benefits of Rust for systems programming?".into(),
                cache_control: None,
            }],
        }],
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
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![ContentBlock::Text {
                    text: (*question).to_string(),
                    cache_control: Some(CacheControl::ephemeral_5m()),
                }],
            }],
            temperature: Some(0.3),
        };

        let response = client.messages().create(request).await?;

        println!("\nQuestion {}: {question}", i + 1);
        println!("Response: ");
        for block in &response.content {
            match block {
                ContentBlock::Text { text, .. } => println!("{text}"),
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
