# CLAUDE.md - agentic-tools-core

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-tools-core
- Path: crates/agentic-tools/core/
- Role: lib
- Family: agentic-tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

`agentic-tools-core` owns canonical tool contracts: `Tool`, `ToolRegistry`, runtime formatting, and the canonical schemars-based MCP schema generator/cache in `src/schema.rs`. When changing schema behavior here, preserve the invariant that canonical generation happens once and any provider or transport compatibility lowering happens later on cloned output.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p agentic-tools-core -- --check
cargo clippy -p agentic-tools-core --all-targets -- -D warnings

# Tests
cargo test -p agentic-tools-core

# Build
cargo build -p agentic-tools-core
```
<!-- END:xtask:autogen -->

## Notes

- `schema::mcp_schema::cached_schema_for` and `cached_output_schema_for` are canonical cache boundaries. Do not mutate the cached `Arc<Schema>` values in place.
- `schema::publication` is the compatibility layer for publication-time lowering on serialized JSON clones. Keep it explicit and opt-in.
- Preserve current nullable semantics from `NullFirstOptional`; publication profiles should not silently strip null branches or rewrite canonical schema structure unless explicitly designed for that purpose.
- If a provider compatibility issue appears, prove the canonical emitted shape first, then add the narrowest boundary-layer lowering that fixes the real failing surface.
