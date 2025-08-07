# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

claudecode_rs is a Rust SDK for programmatically interacting with Claude Code (the Claude CLI). It provides a type-safe, async API to launch Claude sessions, send queries, and handle responses in various formats.

## Common Make Commands

### Quick Commands (Silent by Default)
```bash
make all        # Run check, test, and build (silent)
make check      # Run clippy linting
make test       # Run all tests
make build      # Build release binary
```

### Output Variants
```bash
make test-normal   # Normal output
make test-verbose  # Verbose output
```

### Test Categories
```bash
make test-unit         # Unit tests only
make test-integration  # Integration tests only
make test-ignored      # Run ignored tests
make test-all          # All tests including ignored
```

### Development
```bash
make fmt          # Format code
make fmt-check    # Check formatting
make doc-open     # Generate and open docs
make audit        # Security audit
make outdated     # Check dependencies
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
   - `Model` enum: Sonnet, Opus
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