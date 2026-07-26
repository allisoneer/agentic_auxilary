# CLAUDE.md - attention-turso

<!-- BEGIN:xtask:autogen header -->
- Crate: attention-turso
- Path: crates/attention/turso/
- Role: lib
- Family: attention
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

`attention-turso` provides Attention's exact-pinned native local Turso storage foundation, including lifecycle, migration, commit-outcome, WAL, and stopped backup/restore behavior. Use it for qualified local Turso storage; see [README.md](README.md) for the full supported contract and limitations.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check attention-turso

# Tests
just crate-test attention-turso

# Build
just crate-build attention-turso
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
