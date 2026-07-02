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
just crate-check exa-async

# Tests
just crate-test exa-async

# Build
just crate-build exa-async
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
