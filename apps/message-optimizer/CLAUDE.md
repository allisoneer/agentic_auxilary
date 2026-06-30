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
just crate-check message-optimizer-bin

# Tests
just crate-test message-optimizer-bin

# Build
just crate-build message-optimizer-bin
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
