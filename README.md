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

### 📡 [opencode_rs](opencode_rs/)
Rust SDK for OpenCode: HTTP-first client with SSE streaming and optional managed server/CLI helpers.
- HTTP endpoints with typed request/response models
- SSE event subscriptions with backoff and heartbeats
- Async builder-based client; Unix-only
- Optional managed server startup (feature: server)

### 🔄 [claude_to_opencode_migration](claude_to_opencode_migration/)
Python script to migrate Claude Code configuration to OpenCode.
- Zero-install via uv run (PEP 723 inline deps)
- Dry-run with unified diffs and timestamped backups
- Fine-grained flags: agents, commands, permissions, mcp

### 🧠 [GPT-5 Reasoner](gpt5_reasoner/)
Two-phase prompt optimization tool: optimize with Claude, execute with GPT-5.
- Directory-based file discovery with smart filtering
- Dual CLI and MCP interfaces
- Configurable optimizer model (default: Claude Sonnet 4.5)
- Automatic binary file detection and deduplication

### 🧩 [Anthropic Async](anthropic_async/)
Production-ready asynchronous client for Anthropic's API with prompt caching support.
- Messages API (create, count tokens) and Models API
- Retry with exponential backoff, beta feature support
- Strong typing and examples

### 💬 [PR Comments](pr_comments/)
Fetch GitHub PR comments with resolution filtering.
- CLI + MCP support
- Filter by author, state, and resolution
- Useful for code review analytics and CI reporting

### 🔧 [Coding Agent Tools](coding_agent_tools/)
CLI + MCP tools for coding assistants with gitignore-aware directory listing.
- Dual CLI and MCP interfaces
- Respects .gitignore and built-in ignore patterns
- Implicit pagination for large directories

### 📋 [linear_tools](linear_tools/)
CLI + MCP tools for Linear issue management.
- Search and read Linear issues
- Works as both CLI and MCP server
- Extra fields via LINEAR_TOOLS_EXTRAS environment variable

## Quick Start

```bash
# Clone repository
git clone https://github.com/allisoneer/agentic_auxilary
cd agentic_auxilary

# Build thoughts_tool
cd thoughts_tool && just build

# Build universal_tool
cd universal_tool && just build

# Build claudecode_rs
cd claudecode_rs && just build

# Build gpt5_reasoner
cd gpt5_reasoner && just build
```

## Installation

### Install thoughts_tool from crates.io
```bash
cargo install thoughts-tool
```

<!-- Note: Content between BEGIN:autodeps and END:autodeps is auto-generated. Manual edits inside will be overwritten. -->

### Use universal_tool in your project
<!-- BEGIN:autodeps {"crates":["universal-tool-core","universal-tool-macros"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
universal-tool-core = "0.2.5"
universal-tool-macros = "0.1.8"
```
<!-- END:autodeps -->

### Use claudecode in your project
<!-- BEGIN:autodeps {"crates":["claudecode"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
claudecode = "0.1.9"
```
<!-- END:autodeps -->

### Use opencode_rs in your project
<!-- BEGIN:autodeps {"crates":["opencode_rs"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
opencode_rs = "0.1.1"
```
<!-- END:autodeps -->

### Run claude_to_opencode_migration (no install; via uv)
```bash
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --dry-run
uv run claude_to_opencode_migration/migrate_claude_to_opencode.py --all
```

### Install gpt5_reasoner
```bash
cargo install gpt5_reasoner
```

### Install pr_comments
```bash
cargo install pr_comments
```

### Install coding_agent_tools
```bash
cargo install coding_agent_tools
```

### Install linear-tools
```bash
cargo install linear-tools
```

### Use anthropic-async in your project
<!-- BEGIN:autodeps {"crates":["anthropic-async"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
anthropic-async = "0.2.1"
```
<!-- END:autodeps -->

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
