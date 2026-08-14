# CLAUDE.md - attention-server

<!-- BEGIN:xtask:autogen header -->
- Crate: attention-server
- Path: apps/attention-server/
- Role: app
- Family: attention
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Production composition for Attention protocol v1. This crate maps protocol DTOs to kernel commands,
opens and migrates native Turso storage before binding, owns post-commit publication and one reminder
scheduler, and exposes bounded WebSocket RPC plus generic delivery-worker operations.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check attention-server

# Tests
just crate-test attention-server

# Build
just crate-build attention-server
```
<!-- END:xtask:autogen -->

## Notes

- Preserve startup order: validate/open storage, run migrations, establish durable identity, then bind
  and start the scheduler. Migration failure must not expose a listener.
- Preserve shutdown order: stop admission/connections, drain service work, join the scheduler, close
  Turso, and release directory ownership.
- Keep loopback as the default. Non-loopback operation is an explicit unauthenticated exposure and
  must remain opt-in; browser origins are exact allowlist entries while absent native-client Origin
  is accepted.
- Keep all queues, frames, JSON complexity, replay, claims, and delivery text bounded. Overflow must
  disconnect/resume or return a typed error, never grow without limit.
- Publish only frozen committed events. Mutation receipts and ChangeEvents are not alternate storage
  or delivery authorities; only Outbox rows are send work.
- Do not add provider SDKs, credentials, secret payloads, cloud sync, SQLite/libSQL access, auth
  claims, or exactly-once delivery claims here.
- Logs and protocol errors must remain redacted and must not expose raw frames, canonical source
  content, delivery text, lease/provider identifiers, paths, or internal database errors.
- Operator behavior, supported limits, and explicit failure boundaries belong in `README.md`; update
  it whenever configuration or runtime guarantees change.
