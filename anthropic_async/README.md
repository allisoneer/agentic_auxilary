# anthropic-async

A production-ready Anthropic API client for Rust with prompt caching support.

[![Crates.io](https://img.shields.io/crates/v/anthropic-async.svg)](https://crates.io/crates/anthropic-async)
[![Documentation](https://docs.rs/anthropic-async/badge.svg)](https://docs.rs/anthropic-async)
[![License](https://img.shields.io/crates/l/anthropic-async.svg)](LICENSE)

## Features

- ✅ Full support for Messages API (create, count tokens)
- ✅ Models API (list, get)
- 🚀 Prompt caching with TTL management
- 🔐 Dual authentication (API key or Bearer token)
- 🔄 Automatic retry with exponential backoff
- 🎛️ Beta feature support
- 📝 Comprehensive examples
- 🦀 100% safe Rust with strong typing

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
anthropic-async = "0.1.0"
```

## Quick Start

```rust
use anthropic_async::{Client, types::messages::*};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Client will use ANTHROPIC_API_KEY environment variable
    let client = Client::new();

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 100,
        messages: vec![Message {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "Hello, Claude!".into(),
                cache_control: None,
            }],
        }],
        system: None,
        temperature: None,
    };

    let response = client.messages().create(req).await?;
    println!("{:?}", response.content);
    Ok(())
}
```

## Authentication

The client supports two authentication methods:

### API Key (Primary)

```rust
// From environment variable
let client = Client::new(); // Uses ANTHROPIC_API_KEY

// Explicit
let config = AnthropicConfig::new()
    .with_api_key("sk-ant-api03-...");
let client = Client::with_config(config);
```

### Bearer Token (OAuth/Enterprise)

```rust
// From environment variable
// Set ANTHROPIC_AUTH_TOKEN
let client = Client::new();

// Explicit
let config = AnthropicConfig::new()
    .with_bearer("your-oauth-token");
let client = Client::with_config(config);
```

## Prompt Caching

Reduce costs and latency with prompt caching:

```rust
use anthropic_async::types::common::CacheControl;

let req = MessagesCreateRequest {
    model: "claude-3-5-sonnet".into(),
    max_tokens: 100,
    system: Some(vec![
        ContentBlock::Text {
            text: "You are an expert programmer.".into(),
            cache_control: Some(CacheControl::ephemeral_1h()),
        }
    ]),
    messages: vec![Message {
        role: MessageRole::User,
        content: vec![ContentBlock::Text {
            text: "Explain Rust ownership.".into(),
            cache_control: Some(CacheControl::ephemeral_5m()),
        }],
    }],
    temperature: None,
};
```

### TTL Rules

- Cache entries can have 5-minute or 1-hour TTLs
- When mixing TTLs, 1-hour entries must appear before 5-minute entries
- Minimum cacheable prompt: 1024 tokens (Opus/Sonnet), 2048 (Haiku 3.5)

## Beta Features

Enable beta features using the configuration:

```rust
use anthropic_async::config::BetaFeature;

let config = AnthropicConfig::new()
    .with_beta_features([
        BetaFeature::PromptCaching20240731,
        BetaFeature::ExtendedCacheTtl20250411,
    ]);
```

Or use custom beta strings:

```rust
let config = AnthropicConfig::new()
    .with_beta(vec!["new-beta-feature"]);
```

## Error Handling and Retries

The client automatically retries on:
- 408 Request Timeout
- 409 Conflict
- 429 Rate Limited
- 5xx Server Errors
- 529 Overloaded

Retries use exponential backoff and respect `Retry-After` headers.

```rust
match client.messages().create(req).await {
    Ok(response) => println!("Success: {:?}", response),
    Err(AnthropicError::Api(error)) => {
        println!("API error: {} ({})", error.message, error.r#type.unwrap_or_default());
    }
    Err(e) => println!("Other error: {}", e),
}
```

## Examples

See the `examples/` directory for complete examples:

- `01-basic-completion` - Simple message creation
- `04-model-listing` - List available models
- `05-kitchen-sink` - All features demonstration

Run an example:

```bash
cd anthropic_async/examples/01-basic-completion && cargo run
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT OR Apache-2.0 license.
