# CLAUDE.md - exa-async

<!-- BEGIN:xtask:autogen header -->
- Crate: exa-async
- Path: crates/services/exa-async/
- Role: lib
- Family: services
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Async Rust client for the Exa API.

- Defaults to `EXA_API_KEY` and `EXA_BASE_URL` env vars.
- Typical usage: create a client via `exa_async::Client::new()` (env-based) or `Client::with_config(...)`.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p exa-async -- --check
cargo clippy -p exa-async --all-targets -- -D warnings

# Tests
cargo test -p exa-async

# Build
cargo build -p exa-async
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
