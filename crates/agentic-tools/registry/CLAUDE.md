# CLAUDE.md - agentic-tools-registry

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-tools-registry
- Path: crates/agentic-tools/registry/
- Role: lib
- Family: agentic-tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Builds the unified tool registry used by `agentic-mcp`, applying serialized defaults, allowlist intersection, and runtime-only nested-subagent policy.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check agentic-tools-registry

# Tests
just crate-test agentic-tools-registry

# Build
just crate-build agentic-tools-registry
```
<!-- END:xtask:autogen -->

## Notes

- Plain/default construction preserves the parent OpenCode surface: workspace tools and specialized Thoughts/References readers remain disabled.
- `AgenticRuntimeConfig` is non-serialized and used only by explicit nested profiles to gate workspace read/todo, specialized readers, and canonical CLI sandbox roots.
- Nested policy never enables workspace edit or apply-patch; the exact process allowlist remains an independent publication boundary.
