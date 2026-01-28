# Agentic Auxiliary Tools

Development tools and libraries for agentic workflows (MCP, CLIs, and Rust SDKs).

OpenCode is supported as a first-class workflow in this repo. The codebase uses `just` for task automation and `xtask` for repository maintenance. AI-assisted development is facilitated through per-crate `CLAUDE.md` files containing context and commands.

## Primary Tools

### [`agentic-mcp`](apps/agentic-mcp/)
Unified MCP server for all agentic-tools. Provides a single entry point for accessing the entire agentic-tools toolkit via the Model Context Protocol, enabling seamless integration with AI assistants and coding tools.

### [`thoughts`](apps/thoughts/)
CLI for flexible thought management using filesystem mounts. Unifies documentation across git repositories using mergerfs/FUSE, enabling automatic git synchronization and cross-platform support (Linux/macOS).

### [`claudecode`](crates/services/claudecode-rs/)
A Rust SDK for programmatically interacting with Claude Code. Provides type-safe event streaming, async-first API design, MCP support, and builder pattern configuration for launching and managing Claude sessions.

### [`anthropic-async`](crates/services/anthropic-async/)
Production-ready asynchronous client for Anthropic's API with prompt caching support. Includes Messages API (create, count tokens) and Models API with retry logic, exponential backoff, and strong typing.

### [`gpt5_reasoner`](crates/tools/gpt5-reasoner/)
GPT-5 prompt optimization and execution tool with MCP and CLI interfaces. Implements a two-phase approach: optimize prompts with Claude, then execute with GPT-5.2 xhigh reasoning. Supports directory-based file discovery with smart filtering.

## Quick Start

```bash
# Clone repository
git clone https://github.com/allisoneer/agentic_auxilary
cd agentic_auxilary

# Build entire workspace
just build

# Build a specific crate
just crate-build agentic-mcp
just crate-build thoughts
just crate-build claudecode

# Run tests
just test

# Check formatting and lints
just check
```

## Installation

### Install binaries from source

```bash
cargo install --path apps/agentic-mcp
cargo install --path apps/thoughts
```

### Use claudecode in your project
<!-- BEGIN:autodeps {"crates":["claudecode"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
claudecode = "0.1.9"
```
<!-- END:autodeps -->

### Use anthropic-async in your project
<!-- BEGIN:autodeps {"crates":["anthropic-async"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
anthropic-async = "0.2.1"
```
<!-- END:autodeps -->

### Use opencode_rs in your project
<!-- BEGIN:autodeps {"crates":["opencode_rs"], "fence":"toml", "header":"[dependencies]"} -->
```toml
[dependencies]
opencode_rs = "0.1.1"
```
<!-- END:autodeps -->

## Additional Tools

<!-- BEGIN:xtask:autogen readme-additional-tools -->
### linear

- [`linear-tools`](crates/linear/tools) - Linear issue tools via CLI + MCP

### tools

- [`coding_agent_tools`](crates/tools/coding-agent-tools) - Coding agent tools (CLI + MCP). First tool: ls.
- [`pr_comments`](crates/tools/pr-comments) - Fetch GitHub PR comments via CLI and MCP
- [`thoughts-mcp-tools`](crates/tools/thoughts-mcp-tools) - MCP tool wrappers for thoughts-tool using agentic-tools framework
<!-- END:xtask:autogen -->

## Supporting Libraries

<!-- BEGIN:xtask:autogen readme-supporting-libraries -->
### agentic-tools

- [`agentic-tools-core`](crates/agentic-tools/core) - Core traits and types for agentic-tools library family
- [`agentic-tools-macros`](crates/agentic-tools/macros) - Proc macros for agentic-tools library family
- [`agentic-tools-mcp`](crates/agentic-tools/mcp) - MCP server integration for agentic-tools library family
- [`agentic-tools-napi`](bindings/node/agentic-tools-napi) - N-API bindings for agentic-tools, enabling TypeScript/JavaScript integration
- [`agentic-tools-registry`](crates/agentic-tools/registry) - Unified tool registry aggregating all agentic-tools domain registries
- [`agentic-tools-utils`](crates/agentic-tools/utils) - Shared utilities for agentic-tools ecosystem: pagination, http, secrets, cli

### infra

- [`agentic_logging`](crates/infra/agentic-logging) - Centralized JSONL logging infrastructure for agentic tools
- [`thoughts-tool`](crates/infra/thoughts-core) - Flexible thought management using filesystem mounts for git repositories

### linear

- [`linear-queries`](crates/linear/queries) - Cynic queries and input types for Linear
- [`linear-schema`](crates/linear/schema) - Cached Linear GraphQL schema for cynic

### services

- [`opencode_rs`](crates/services/opencode-rs) - Rust SDK for OpenCode (HTTP-first hybrid with SSE streaming)
<!-- END:xtask:autogen -->

## Development

### Task Automation

This repository uses `just` for task automation. Common commands:

```bash
just check          # Check entire workspace (fmt + clippy)
just test           # Test entire workspace
just build          # Build entire workspace
just fmt            # Format entire workspace

# Per-crate commands
just crate-check <crate>    # Run formatting and clippy checks for a crate
just crate-test <crate>     # Run tests for a crate
just crate-build <crate>    # Build a crate
```

### Repository Maintenance (xtask)

The `xtask` crate provides repository maintenance tooling:

```bash
just xtask-sync         # Sync autogen content (CLAUDE.md, release-plz.toml, README.md)
just xtask-verify       # Verify metadata, policy, and file freshness
just xtask-sync-check   # Check if sync is needed (for CI)
just xtask-verify-check # Full verification including generated files
```

### AI-Assisted Development

Each crate has a `CLAUDE.md` file with crate-specific context and commands. The root `CLAUDE.md` provides repository-wide guidance.

## Legacy

<!-- BEGIN:xtask:autogen readme-legacy -->
### legacy

- [`universal-tool-core`](crates/legacy/universal-tool-core) - DEPRECATED: Use agentic-tools-* crates and agentic-mcp instead. Core runtime library for Universal Tool Framework.
- [`universal-tool-integration-tests`](crates/legacy/universal-tool-integration-tests) - universal-tool-integration-tests
- [`universal-tool-macros`](crates/legacy/universal-tool-macros) - DEPRECATED: Use agentic-tools-* crates and agentic-mcp instead. Procedural macros for Universal Tool Framework.
<!-- END:xtask:autogen -->

## License

MIT - See [LICENSE](LICENSE)
