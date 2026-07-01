# CLAUDE.md - message_optimizer

<!-- BEGIN:xtask:autogen header -->
- Crate: message_optimizer
- Path: crates/tools/message-optimizer/
- Role: tool-lib
- Family: tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Standalone Anthropic-backed library for optimizing a raw message into GPT-5.4 prompt components. It validates model output, retries output-contract violations, and assembles the final system and user prompts into a single rendered prompt.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check message_optimizer

# Tests
just crate-test message_optimizer

# Build
just crate-build message_optimizer
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
