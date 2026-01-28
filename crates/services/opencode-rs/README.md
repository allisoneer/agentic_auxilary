# opencode_rs

Rust SDK for OpenCode (HTTP-first hybrid with SSE streaming). Provides an async client for HTTP endpoints and Server-Sent Events (SSE) subscriptions, with optional helpers for managing a local server and CLI integration.

> Platform: Unix-like systems (Linux/macOS). Windows is currently not supported.

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
  - [Basic Session](#basic-session)
  - [Streaming Events](#streaming-events)
- [Features](#features)
- [Configuration](#configuration)
- [Error Handling](#error-handling)
- [Examples](#examples)
- [License](#license)

## Installation

Add the crate to your Cargo.toml:

```toml
[dependencies]
opencode_rs = "0.1.0"
```

Feature flags:

- Default: `http`, `sse`
- Optional: `server`, `cli`
- Convenience: `full` = `http` + `sse` + `server` + `cli`

Examples:

```bash
# Minimal (HTTP + SSE)
cargo build

# Explicitly choose features
cargo build --no-default-features --features http
cargo build --features "http sse"
cargo build --features full
```

## Usage

All examples assume an OpenCode server is running locally:

```bash
opencode serve
# Default URL: http://127.0.0.1:4096
```

### Basic Session

Create a session and send a prompt via HTTP:

```rust
use opencode_rs::ClientBuilder;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::session::CreateSessionRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build client connecting to default localhost:4096
    let client = ClientBuilder::new().build()?;

    // Create a new session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await?;
    println!("Created session: {}", session.id);

    // Send a prompt
    client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Hello OpenCode! What can you help me with?".into(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: None,
                agent: None,
                no_reply: None,
                system: None,
                variant: None,
            },
        )
        .await?;

    println!("Prompt sent successfully!");

    // Clean up session to avoid accumulating dangling sessions
    client.sessions().delete(&session.id).await?;
    println!("Session deleted");

    Ok(())
}
```

### Streaming Events

Subscribe to SSE and stream events in real time:

```rust
use opencode_rs::ClientBuilder;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::session::CreateSessionRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt::init();

    // Build client
    let client = ClientBuilder::new().build()?;

    // Create session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await?;
    println!("Created session: {}", session.id);

    // Subscribe to session events BEFORE sending prompt
    let mut subscription = client.subscribe_session(&session.id).await?;
    println!("Subscribed to events");

    // Send prompt
    client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Write a haiku about Rust programming".into(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: None,
                agent: None,
                no_reply: None,
                system: None,
                variant: None,
            },
        )
        .await?;
    println!("Prompt sent, streaming events...\n");

    // Stream events until session is idle or error
    loop {
        match subscription.recv().await {
            Some(Event::SessionIdle { .. }) => {
                println!("\n[Session completed]");
                break;
            }
            Some(Event::SessionError { properties }) => {
                eprintln!("\n[Session error: {:?}]", properties.error);
                break;
            }
            Some(Event::MessagePartUpdated { properties }) => {
                if let Some(delta) = &properties.delta {
                    print!("{}", delta);
                }
            }
            Some(Event::ServerHeartbeat { .. }) => {
                // Heartbeat received, connection alive
            }
            Some(event) => {
                println!("[Event: {:?}]", event);
            }
            None => {
                println!("[Stream closed]");
                break;
            }
        }
    }

    // Cleanup
    client.sessions().delete(&session.id).await?;
    println!("Session deleted");

    Ok(())
}
```

## Features

- HTTP-first design for creating sessions and sending prompts
- SSE streaming with heartbeat and retry/backoff (via `reqwest-eventsource` + `backoff`)
- Async API built on Tokio
- Optional managed server launcher and CLI integration (feature flags)
- Strongly-typed request/response and event enums

## Configuration

Use `ClientBuilder` to construct a client. Defaults connect to `http://127.0.0.1:4096`. Builder options allow customizing base URL, timeouts, and SSE behavior (e.g., retry/backoff). See docs.rs for full builder options.

Example (defaults):
```rust
let client = opencode_rs::ClientBuilder::new().build()?;
```

## Error Handling

Most operations return `Result<T, opencode_rs::Error>`. Common error categories include HTTP status errors, JSON (de)serialization errors, SSE connection/stream errors, and timeouts.

```rust
match client.sessions().delete(&session.id).await {
    Ok(_) => println!("Session deleted"),
    Err(e) => eprintln!("Failed to delete session: {e}"),
}
```

## Examples

Run the included examples (requires a local OpenCode server):
```bash
# Basic HTTP example
cargo run --example basic

# SSE streaming example
cargo run --example streaming
```

## License

Licensed under the Apache-2.0 License. See the crate's Cargo.toml for details.
