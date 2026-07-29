# CLAUDE.md - thoughts-mcp-tools

<!-- BEGIN:xtask:autogen header -->
- Crate: thoughts-mcp-tools
- Path: crates/tools/thoughts-mcp-tools/
- Role: tool-lib
- Family: tools
- Integrations: mcp=false, logging=true, napi=false
<!-- END:xtask:autogen -->

## Overview

Thoughts and References MCP tools, including runtime-only bounded specialized readers for nested subagents.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check thoughts-mcp-tools

# Tests
just crate-test thoughts-mcp-tools

# Build
just crate-build thoughts-mcp-tools
```
<!-- END:xtask:autogen -->

## Notes

- `thoughts_read_document` and `thoughts_read_reference` are absent by default and enabled independently only by nested runtime policy.
- Readers canonicalize against resolved active-work or References bases, reject traversal/symlink escape, and share workspace read bounds/rendering.
- Nested read-only mode skips environment synchronization to avoid side effects; the caller validates canonical roots before serving.
