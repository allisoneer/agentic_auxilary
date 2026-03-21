# CLAUDE.md - message-optimizer-bin

<!-- BEGIN:xtask:autogen header -->
- Crate: message-optimizer-bin
- Path: apps/message-optimizer/
- Role: app
- Family: tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Thin CLI for optimizing a single message into GPT-5.4 prompt components. Pass the message as an argument or `-` to read from stdin, and use `--json` or `--pretty` for structured output.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p message-optimizer-bin -- --check
cargo clippy -p message-optimizer-bin --all-targets -- -D warnings

# Tests
cargo test -p message-optimizer-bin

# Build
cargo build -p message-optimizer-bin
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
