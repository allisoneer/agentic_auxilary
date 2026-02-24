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

# Record new API cassettes and update insta snapshots
just record-snapshots

# Record only multi-turn conformance test
just record-multi-turn

# Run only multi-turn conformance test against live API
just live-multi-turn

# Review and accept insta snapshots interactively
just insta-review
```

## Live vs Replay Testing

Tests use environment variables to control mode:

| Variable | Purpose |
|----------|---------|
| `ANTHROPIC_LIVE=1` | Run against real API (requires `ANTHROPIC_API_KEY`) |
| `ANTHROPIC_RECORD=1` | Record API interactions to YAML cassettes (use with `ANTHROPIC_LIVE=1`) |
| `INSTA_UPDATE=always` | Auto-accept insta snapshot changes |

**Modes:**
- **Replay mode** (default): Uses httpmock to replay recorded cassettes. No API key needed. Runs in CI.
- **Live mode** (`ANTHROPIC_LIVE=1`): Makes real API calls. Requires `ANTHROPIC_API_KEY`.
- **Record mode** (`ANTHROPIC_LIVE=1 ANTHROPIC_RECORD=1`): Makes real API calls and saves responses to `tests/snapshots/*.yaml`.

**Snapshot files:**
- `tests/snapshots/*.yaml` - httpmock cassettes (recorded API interactions)
- `tests/snapshots/*.snap` - insta snapshots (response structure assertions)
