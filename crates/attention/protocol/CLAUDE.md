# CLAUDE.md - attention-protocol

<!-- BEGIN:xtask:autogen header -->
- Crate: attention-protocol
- Path: crates/attention/protocol/
- Role: lib
- Family: attention
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

`attention-protocol` defines strict Serde wire DTOs for the domain-neutral Attention JSON-RPC foundation. Use crate-root `RpcRequest`, `RpcNotification`, and `RpcResponse` types for envelopes; `HelloRequest` and `HelloResult` for `rpc.hello` negotiation; and the exported ID, version, and `WireTimestamp` types for protocol values.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check attention-protocol

# Tests
just crate-test attention-protocol

# Build
just crate-build attention-protocol
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
