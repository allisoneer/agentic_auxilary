# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

claudecode_rs is a Rust SDK for programmatically interacting with Claude Code (the Claude CLI). It provides a type-safe, async API to launch Claude sessions, send queries, and handle responses in various formats.

## Common Commands

### Quick Commands (Silent by Default)
```bash
just check      # Run formatting and clippy checks
just test       # Run all tests
just build      # Build release binary
just fmt        # Format code
just fmt-check  # Check formatting
```

### Output Variants
```bash
OUTPUT_MODE=normal just test   # Normal output
OUTPUT_MODE=verbose just test  # Verbose output
```

### Cargo Direct Commands
```bash
cargo test --lib               # Unit tests only (no claude CLI required)
cargo test -- --ignored        # Run ignored tests
```

## Commands

### Build and Development
```bash
# Build the project
cargo build

# Build with all features (including examples)
cargo build --all-features

# Run tests (note: integration tests require claude CLI installed)
cargo test

# Run only unit tests (no claude CLI required)
cargo test --lib

# Run a specific test
cargo test test_name

# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy lints
cargo clippy
```

### Running Examples
```bash
# Basic query example
cargo run --example basic

# Streaming events example
cargo run --example streaming

# MCP (Model Context Protocol) example
cargo run --example mcp

# Debug streaming example
cargo run --example streaming_debug
```

## Architecture

### Core Components

1. **Client** (`src/client.rs`): Main entry point for launching Claude sessions. Handles finding the claude executable and spawning processes.

2. **Session** (`src/session.rs`): Manages running Claude processes and handles different output formats (Text, JSON, StreamingJSON). Implements automatic resource cleanup.

3. **Configuration** (`src/config.rs`): 
   - `SessionConfig`: Builder pattern for session configuration
   - `MCPConfig`: Model Context Protocol server configuration
   - Supports resume sessions, max turns, custom prompts, tool filtering

4. **Type System** (`src/types.rs`):
   - `Model` enum: Sonnet, Opus, Haiku
   - `OutputFormat` enum: Text, Json, StreamingJson
   - Event types for streaming: Assistant, System, Result, Error
   - Strongly-typed message and content structures

5. **Stream Parsing** (`src/stream.rs`): Format-specific parsers:
   - `JsonStreamParser`: NDJSON event streaming
   - `SingleJsonParser`: Complete JSON responses
   - `TextParser`: Plain text output

### Key Design Patterns

- **Builder Pattern**: SessionConfigBuilder for fluent configuration
- **Type-safe Events**: Enum-based event system prevents runtime parsing errors
- **RAII**: ProcessHandle implements Drop for automatic cleanup
- **Error Handling**: Comprehensive ClaudeError enum with context
- **Async-first**: All APIs are async using Tokio runtime

### Testing Approach

- Unit tests are embedded in modules using `#[cfg(test)]`
- Integration tests in `tests/` directory require claude CLI
- Tests marked with `#[ignore]` need manual execution with `cargo test -- --ignored`
- All tests check for Claude CLI availability before running