# CLAUDE.md - agentic-mcp

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-mcp
- Path: apps/agentic-mcp/
- Role: app
- Family: agentic-tools
- Integrations: mcp=true, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Unified stdio MCP server for parent OpenCode sessions and least-privilege nested `ask_agent` sessions.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check agentic-mcp

# Tests
just crate-test agentic-mcp

# Build
just crate-build agentic-mcp
```
<!-- END:xtask:autogen -->

## Nested profiles

`--nested-profile codebase|thoughts|references|web` is runtime-only and must be paired with an exact `--allow` list. Omitting the allowlist publishes zero tools. Profiles derive canonical sandbox roots and independently gate workspace read/todo and specialized Thoughts/References readers. They never enable workspace edit/apply-patch. Thoughts/References nested read-only tools skip side-effectful readiness synchronization because their roots are validated before serving.

Nested allowlist names are exact and case-sensitive. Empty, duplicate, whitespace-padded, qualified, prefixed/suffixed, unknown, or out-of-profile names make the entire nested publication fail closed with zero tools. Server-config allowlists and convenience flags never substitute for an explicitly valid nested `--allow` value.

Plain startup has no nested runtime gates and preserves parent behavior. Workspace and specialized readers remain default-disabled. `--list-tools` and startup diagnostics use stderr; stdout is reserved for MCP protocol frames.
