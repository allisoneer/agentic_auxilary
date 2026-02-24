# CLAUDE.md - anthropic-async

<!-- BEGIN:xtask:autogen header -->
- Crate: anthropic-async
- Path: crates/services/anthropic-async/
- Role: lib
- Family: services
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Briefly describe the purpose of this crate and how to use it.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p anthropic-async -- --check
cargo clippy -p anthropic-async --all-targets -- -D warnings

# Tests
cargo test -p anthropic-async

# Build
cargo build -p anthropic-async
```
<!-- END:xtask:autogen -->

## Notes

Anthropic API client for Rust with support for:
- Messages API with tool calling and multi-turn conversations
- Extended thinking (ThinkingConfig)
- Streaming with SSE accumulation
- Prompt caching with TTL validation

## Crate-Specific Commands

This crate has a local justfile for specialized testing. Run from this directory:

```bash
# Run conformance tests against live Anthropic API (requires ANTHROPIC_API_KEY)
just live-test

# Record new API snapshots for conformance tests
just record-snapshots

# Run only multi-turn conformance test
just live-multi-turn
```

## Live vs Replay Testing

Tests use `ANTHROPIC_LIVE=1` to switch between modes:
- **Replay mode** (default): Uses wiremock to replay recorded snapshots. No API key needed. Runs in CI.
- **Live mode**: Makes real API calls. Requires `ANTHROPIC_API_KEY`. Use to record new snapshots or verify against real API.
