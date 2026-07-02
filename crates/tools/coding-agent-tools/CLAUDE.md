# CLAUDE.md - coding_agent_tools

## Purpose
CLI + MCP tools for coding assistants. Currently implements `ls` tool for gitignore-aware directory listing.

## Quick Commands
```bash
just check        # Run formatting and clippy checks
just test         # Run tests
just build        # Build the project
just fmt          # Format code
```

## Architecture
- `src/main.rs` - CLI entry point with clap subcommands (ls, mcp)
- `src/lib.rs` - Tool router with universal_tool macros
- `src/types.rs` - Depth, Show, LsOutput, LsEntry types
- `src/paths.rs` - Path normalization utilities
- `src/walker.rs` - Directory traversal with ignore/globset
- `src/pagination.rs` - Implicit pagination state for MCP

## Key Design Decisions
- `ignore` crate with `parents(false)` for gitignore-aware traversal
- `globset` for custom ignore patterns (not `add_ignore()`)
- Pagination state in struct; CLI creates fresh instance (no pagination), MCP reuses Arc-wrapped instance
- McpFormatter for token-efficient text output

## Search Ignore Policy

- `cli_glob` and `cli_grep` apply default ignores from gitignore plus built-in/common directories such as `node_modules/`, `target/`, and `logs/`.
- If an investigation expects matches inside ignored paths, retry the same request with `include_ignored=true`.
- `include_hidden` stays independent from `include_ignored`; hidden files still require their own flag.

<!-- BEGIN:xtask:autogen header -->
- Crate: coding_agent_tools
- Path: crates/tools/coding-agent-tools/
- Role: tool-lib
- Family: tools
- Integrations: mcp=false, logging=true, napi=false
<!-- END:xtask:autogen -->

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check coding_agent_tools

# Tests
just crate-test coding_agent_tools

# Build
just crate-build coding_agent_tools
```
<!-- END:xtask:autogen -->
