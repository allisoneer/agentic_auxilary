# CLAUDE.md - attention-client

<!-- BEGIN:xtask:autogen header -->
- Crate: attention-client
- Path: crates/attention/client/
- Role: lib
- Family: attention
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Typed, bounded, reconnecting WebSocket SDK for the Attention protocol. Start a
client with `Client::connect`, consume snapshot/change/issue subscriptions,
observe connection status, acknowledge applied cursors, and call `Client::close`
for an orderly shutdown.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check attention-client

# Tests
just crate-test attention-client

# Build
just crate-build attention-client
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
