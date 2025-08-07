# claudecode_rs

A Rust SDK for programmatically interacting with [Claude Code](https://claude.ai/code), providing a type-safe, asynchronous API to launch Claude sessions, send queries, and handle responses in various formats.

## Table of Contents

- [Installation](#installation)
- [Usage](#usage)
  - [Basic Query](#basic-query)
  - [Streaming Events](#streaming-events)
- [Features](#features)
- [Configuration](#configuration)
  - [Session Configuration](#session-configuration)
  - [MCP Configuration](#mcp-configuration)
- [Error Handling](#error-handling)
- [Contributing](#contributing)
- [License](#license)

## Installation

To use `claudecode_rs` in your Rust project, add the following to your `Cargo.toml`:

```toml
[dependencies]
claudecode = "0.1.0"
```

**Note:** If the crate is not yet published on [crates.io](https://crates.io), you can include it via a git repository or local path:

```toml
[dependencies]
claudecode = { git = "https://github.com/AdjectiveAllison/claudecode_rs" }
```

Ensure the Claude CLI is installed and available in your system's PATH. Verify this by running:

```bash
which claude
```

## Usage

### Basic Query

Send a simple query to Claude and print the response:

```rust
use claudecode::{Client, SessionConfig, Model};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new().await?;
    
    let config = SessionConfig::builder("What is the capital of France?")
        .model(Model::Sonnet)
        .build()?;
    
    let result = client.launch_and_wait(config).await?;
    
    if let Some(content) = result.content {
        println!("Claude says: {}", content);
    }
    
    Ok(())
}
```

### Streaming Events

Receive real-time events from Claude using streaming JSON output:

```rust
use claudecode::{Client, SessionConfig, OutputFormat, Event};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new().await?;
    
    let config = SessionConfig::builder("Tell me a story")
        .output_format(OutputFormat::StreamingJson)
        .build()?;
    
    let mut session = client.launch(config).await?;
    
    if let Some(mut events) = session.take_event_stream() {
        while let Some(event) = events.recv().await {
            match event {
                Event::Assistant(msg) => {
                    for content in &msg.message.content {
                        if let Content::Text { text } = content {
                            print!("{}", text);
                        }
                    }
                }
                Event::Result(result) => {
                    if let Some(cost) = result.total_cost_usd {
                        println!("\nTotal cost: ${:.4}", cost);
                    }
                }
                _ => {}
            }
        }
    }
    
    let result = session.wait().await?;
    Ok(())
}
```

## Features

- **Asynchronous API** using the Tokio runtime for non-blocking operations.
- **Type-safe event handling** with enums for different event types.
- **Multiple output formats**: Text, JSON, and Streaming JSON.
- **Flexible session configuration** including model selection, max turns, and system prompts.
- **Model Context Protocol (MCP) integration** for extended functionality.
- **Automatic resource cleanup** and robust error handling.

## Configuration

### Session Configuration

Use `SessionConfig::builder` to configure Claude sessions:

```rust
let config = SessionConfig::builder("Your query here")
    .model(Model::Opus)
    .output_format(OutputFormat::Json)
    .max_turns(5)
    .verbose(true)
    .build()?;
```

Available options include:
- `model`: `Model::Sonnet` or `Model::Opus`
- `output_format`: `OutputFormat::Text`, `OutputFormat::Json`, or `OutputFormat::StreamingJson`
- `max_turns`: Limit interaction turns
- `system_prompt`: Custom system prompt
- `append_system_prompt`: Append to existing system prompt
- `custom_instructions`: Provide custom instructions for the session
- `allowed_tools` / `disallowed_tools`: Control which tools Claude can use
- `verbose`: Enable verbose output

### MCP Configuration

Use `MCPConfig` to configure Model Context Protocol servers:

```rust
use std::collections::HashMap;

let mut servers = HashMap::new();
servers.insert(
    "calculator".to_string(),
    MCPServer {
        command: "npx".to_string(),
        args: vec!["@modelcontextprotocol/server-calculator".to_string()],
        env: None,
    },
);

let mcp_config = MCPConfig {
    mcp_servers: servers,
};

let config = SessionConfig::builder("What is 42 * 17?")
    .mcp_config(mcp_config)
    .build()?;
```

### Session Management

Sessions can be controlled after launching:

```rust
let mut session = client.launch(config).await?;

// Check if session is running
if session.is_running().await {
    // Send interrupt signal (graceful shutdown on Unix)
    session.interrupt().await?;
    
    // Or forcefully kill the process
    session.kill().await?;
}

// Wait for completion
let result = session.wait().await?;
```

## Error Handling

The SDK uses `Result<T, ClaudeError>` for operations that can fail. The `ClaudeResult` type includes an `is_error` flag and an optional `error` message:

```rust
match client.launch_and_wait(config).await {
    Ok(result) => {
        if result.is_error {
            eprintln!("Error: {}", result.error.unwrap_or_default());
        } else {
            println!("Success: {}", result.content.unwrap_or_default());
        }
    }
    Err(e) => eprintln!("SDK Error: {}", e),
}
```

## Contributing

Contributions are welcome! See [CLAUDE.md](CLAUDE.md) for guidelines on building, testing, and contributing to this project.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.