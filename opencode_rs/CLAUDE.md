# CLAUDE.md

This file provides guidance to Claude Code when working with this tool.

## Overview

opencode_rs is a Rust SDK for OpenCode (HTTP-first hybrid with SSE streaming). It provides async APIs for HTTP and SSE interactions, with optional server and CLI features.

## Common Commands

### Quick Commands (Silent by Default)
```bash
just check      # Run formatting and clippy checks (warnings are failures)
just test       # Run all tests
just build      # Build the crate
just fmt        # Format code
just fmt-check  # Check formatting
```

### Output Variants
```bash
OUTPUT_MODE=normal just test   # Normal cargo output
OUTPUT_MODE=verbose just test  # Verbose output
```

### Cargo Direct Commands
```bash
cargo test --lib               # Unit tests only
cargo test -- --ignored        # Run ignored tests if present
cargo clippy -- -D warnings    # Lint as error on warnings
```

## Features

From Cargo.toml:

- default = ["http", "sse"]
- http = ["dep:reqwest", "dep:serde_json"]
- sse = ["dep:reqwest-eventsource", "dep:backoff"]
- server = ["tokio/process", "dep:portpicker"]
- cli = ["tokio/process"]
- full = ["http", "sse", "server", "cli"]

Examples:
```bash
cargo build --features full
cargo test --features http
```

## Notes

- This crate follows the monorepo standard tool Justfile. OUTPUT_MODE controls verbosity via tools/cargo-wrap.sh.
- Typical workflow: just fmt -> just check -> just test -> just build
