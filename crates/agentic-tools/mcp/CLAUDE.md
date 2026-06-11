# CLAUDE.md - agentic-tools-mcp

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-tools-mcp
- Path: crates/agentic-tools/mcp/
- Role: lib
- Family: agentic-tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

`agentic-tools-mcp` wraps `ToolRegistry` with an rmcp server. `RegistryServer::list_tools()` publishes `inputSchema` for all tools and publishes `outputSchema` only in `OutputMode::Structured` when the tool output schema is object-shaped.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p agentic-tools-mcp -- --check
cargo clippy -p agentic-tools-mcp --all-targets -- -D warnings

# Tests
cargo test -p agentic-tools-mcp

# Build
cargo build -p agentic-tools-mcp
```
<!-- END:xtask:autogen -->

## Notes

- MCP schema publication is explicit: `SchemaPublicationProfile::Canonical` publishes the canonical serialized input schema unchanged, while `InlineLocalRefs` lowers only published `inputSchema` JSON.
- Do not apply input-schema publication lowering to `outputSchema` unless separately justified; structured output gating behavior must remain unchanged.
- When debugging provider compatibility issues, compare canonical `list_tools()` output against profiled publication output before changing core schema generation.
- Typical opt-in usage: `RegistryServer::new(registry).with_schema_publication_profile(SchemaPublicationProfile::InlineLocalRefs)`.
