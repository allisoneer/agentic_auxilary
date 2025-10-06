# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Universal Tool Framework (UTF) is a Rust procedural macro library that generates boilerplate code for exposing business logic through multiple interfaces (CLI, REST API, MCP servers). It uses compile-time code generation to create interface-specific methods from a single tool definition.

## Common Development Commands

### Building and Testing
```bash
# Default targets (silent if successful)
make check      # Run formatting and clippy checks
make test       # Run all tests
make build      # Build the project

# Build entire workspace with all features
cargo build --workspace --all-features

# Build only the core library
cargo build -p universal-tool-core

# Build only the macros
cargo build -p universal-tool-macros

# Build specific example
cargo run --example 01-cli-simple
```

### Testing
```bash
# Run all tests in workspace
cargo test --workspace

# Run tests with all features enabled
cargo test --workspace --all-features

# Run tests for specific crate
cargo test -p universal-tool-core
cargo test -p universal-tool-macros

# Run a specific test
cargo test --workspace test_name
```

### Checking and Linting
```bash
# Type check entire workspace
cargo check --workspace --all-features

# Run clippy for linting
cargo clippy --workspace --all-features

# Format code
cargo fmt --all

# Clean build artifacts
cargo clean
```

### Running Examples
```bash
# CLI example
cargo run --example 01-cli-simple -- add 5 3

# REST example (starts server)
cargo run --example 03-rest-simple

# MCP example
cargo run --example 05-mcp-basic

# Kitchen sink (all interfaces)
cargo run --example 06-kitchen-sink
```

## Architecture Overview

### Core Components

1. **universal-tool-macros** (`/universal-tool-macros/src/`)
   - Entry point: `lib.rs:35` - `universal_tool_router` proc macro
   - Parser: `parser.rs:18-50` - Converts syn AST to internal model
   - Model: `model.rs:10-50` - Internal representation (RouterDef, ToolDef, ParamDef)
   - Code generation: `/codegen/` directory with interface-specific generators

2. **universal-tool-core** (`/universal-tool-core/src/`)
   - Runtime support library with feature-gated interface modules
   - Unified error handling: `error.rs:6-30` - `ToolError` type
   - Re-exports macros through `lib.rs:27`

### Code Generation Flow

1. `#[universal_tool_router]` macro processes impl blocks at compile time
2. Parser converts attributes and methods to internal model (`parser.rs:35`)
3. Model validation ensures correctness (`parser.rs:38`)
4. Interface-specific generators create methods:
   - `create_cli_command()` - Creates clap command structure
   - `execute_cli()` - Handles CLI execution
   - `create_rest_router()` - Creates axum router
   - `handle_mcp_call()` - Handles MCP method dispatch

### Key Design Principles

- **Library Pattern**: UTF generates methods users call, never owns main()
- **Feature Gating**: Interfaces are optional via Cargo features
- **Type Safety**: All parameters must implement Serialize/Deserialize/JsonSchema
- **Zero Runtime Overhead**: All code generation happens at compile time

### Working with the Macros

When modifying macro code:
1. Changes to `universal-tool-macros` require rebuilding dependent crates
2. Use `cargo expand` to debug generated code
3. Model types in `model.rs` are the canonical representation
4. Code generators in `/codegen/` should produce idiomatic interface code

### Error Handling Pattern

All interfaces use unified `ToolError` type with:
- Consistent error codes (`ErrorCode` enum)
- Interface-specific error translation in generated code
- Structured error responses for each interface

### Feature Dependencies

- `cli` feature: Enables clap integration
- `rest` feature: Enables axum/tower integration  
- `mcp` feature: Enables MCP/JSON-RPC support
- Features propagate from core to macros crate

## Testing Strategy

- Unit tests in each crate test individual components
- Integration tests in examples demonstrate real usage
- Each example has its own Cargo.toml with specific dependencies
- Test new macro features by adding cases to examples

### Proc-macro integration test pattern

- **Do NOT add [dev-dependencies] to universal-tool-macros** to avoid circular dev-deps with universal-tool-core.
- Cross-crate integration tests live in **universal-tool-integration-tests** (publish = false).
- This follows the pattern used by serde, tokio, etc.: macro crates remain dependency-light; tests that exercise generated code are hosted in a separate test crate.

**Test locations:**
- `universal-tool-macros/tests/` - Compile-time tests only (no runtime dependencies)
- `universal-tool-integration-tests/tests/` - Integration tests requiring both macros and core

**To add a new integration test:**
Create test file in `universal_tool/universal-tool-integration-tests/tests/` and run `make test-normal`

## Important Files to Know

- `/universal-tool-macros/src/parser.rs` - Attribute parsing logic
- `/universal-tool-macros/src/model.rs` - Internal data model
- `/universal-tool-macros/src/codegen/shared.rs` - Shared code generation utilities
- `/universal-tool-core/src/error.rs` - Error types used across all interfaces
- `/examples/06-kitchen-sink/` - Most comprehensive example showing all features