# CLAUDE.md - workspace_tools

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-workspace-tools
- Path: crates/tools/workspace-tools/
- Role: tool-lib
- Family: tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Repository-owned workspace read, ephemeral todo, edit, and apply-patch tools. All are independently disabled by default.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check agentic-workspace-tools

# Tests
just crate-test agentic-workspace-tools

# Build
just crate-build agentic-workspace-tools
```
<!-- END:xtask:autogen -->

## Notes

- `workspace_read` is workspace-contained, rejects symlink/traversal escapes, files over 20 MiB, page limits over 10,000, binary/invalid UTF-8 data, and truncates rendered lines to 2,000 characters.
- `workspace_todowrite` replaces one process-local list. It accepts at most 100 unique nonempty items, with typed status/priority and 4,096 UTF-8 bytes per content string. State resets when the MCP process exits.
- Nested subagent profiles may enable read or todo only. They never enable edit or apply-patch.
