# CLAUDE.md - review-agent-mcp

<!-- BEGIN:xtask:autogen header -->
- Crate: review-agent-mcp
- Path: apps/review-agent-mcp/
- Role: app
- Family: tools
- Integrations: mcp=true, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

review-agent-mcp is an MCP server exposing a single `spawn` tool that launches a sandboxed Claude Opus sub-agent to review local git changes from a prepared `./review.diff` file. Each invocation targets one of four review lenses and validates the structured JSON ReviewReport for schema conformance and source-file line grounding.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p review-agent-mcp -- --check
cargo clippy -p review-agent-mcp --all-targets -- -D warnings

# Tests
cargo test -p review-agent-mcp

# Build
cargo build -p review-agent-mcp
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
