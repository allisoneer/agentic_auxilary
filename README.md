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

### 🧠 [GPT-5 Reasoner](gpt5_reasoner/)
Two-phase prompt optimization tool: optimize with Claude, execute with GPT-5.
- Directory-based file discovery with smart filtering
- Dual CLI and MCP interfaces
- Configurable optimizer model (default: Claude Sonnet 4.5)
- Automatic binary file detection and deduplication

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

# Build gpt5_reasoner
cd gpt5_reasoner && make build
```

## Installation

### Install thoughts_tool from crates.io
```bash
cargo install thoughts-tool
```

### Use universal_tool in your project
```toml
[dependencies]
universal-tool-core = "0.2.1"
universal-tool-macros = "0.1.4"
```

### Use claudecode in your project
```toml
[dependencies]
claudecode = "0.1.4"
```

### Install gpt5_reasoner
```bash
cargo install gpt5_reasoner
```

### Install pr_comments
```bash
cargo install pr_comments
```

Quick example (ClaudeCode):
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
