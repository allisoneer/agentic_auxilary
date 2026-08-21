# CLAUDE.md - attention-desktop

<!-- BEGIN:xtask:autogen header -->
- Crate: attention-desktop
- Path: apps/attention-desktop/src-tauri/
- Role: app
- Family: attention
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Native Tauri v2 shell for Attention. Rust exclusively owns the public `attention-client`
supervisor and exposes only sanitized presentation state and ordered acknowledgement IPC to
the React webview.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check attention-desktop

# Tests
just crate-test attention-desktop

# Build
just crate-build attention-desktop
```
<!-- END:xtask:autogen -->

## Notes

- Keep server networking, resume cursors, and source/provider identities in Rust.
- Do not add shell, filesystem, HTTP, arbitrary network, or broad window capabilities.
- Frontend code may access Tauri only through `src/bridge.ts`.
- Bundling remains disabled; verify native builds with `bun tauri build --debug --no-bundle`.
