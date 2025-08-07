# Agentic Auxiliary Tools

Development tools for enhanced AI agent workflows.

## Tools

### 🔀 [Thoughts Tool](thoughts_tool/)
Unified filesystem for organizing documentation across git repositories using mergerfs/fuse-t.
- Merge multiple repositories into single mountpoint
- Automatic git synchronization
- Cross-platform (Linux/macOS)

### 🛠️ [Universal Tool Framework](universal_tool/)
Write tool logic once, deploy as CLI/REST/MCP without code changes.
- Zero-overhead code generation
- Type-safe interfaces
- Framework agnostic

### 🤖 [ClaudeCode-RS](claudecode_rs/)
Rust SDK for programmatically interacting with Claude Code CLI.
- Type-safe event streaming
- Async-first API design
- MCP (Model Context Protocol) support
- Builder pattern configuration

## Quick Start

```bash
# Clone repository
git clone https://github.com/allisoneer/agentic_auxilary
cd agentic_auxilary

# Build thoughts_tool
cd thoughts_tool && make build

# Build universal_tool
cd universal_tool && cargo build --workspace

# Build claudecode_rs
cd claudecode_rs && make build
```

## Installation

### Install thoughts_tool from crates.io
```bash
cargo install thoughts-tool
```

### Use universal_tool in your project
```toml
[dependencies]
universal-tool-core = "0.1"
universal-tool-macros = "0.1"
```

### Use claudecode in your project
```toml
[dependencies]
claudecode = "0.1"
```

Quick example:
```rust
use claudecode::{Client, SessionConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new().await?;
    let config = SessionConfig::builder("Hello, Claude!")
        .build()?;
    let result = client.launch_and_wait(config).await?;
    println!("{:?}", result.content);
    Ok(())
}
```

## License
MIT - See [LICENSE](LICENSE)
