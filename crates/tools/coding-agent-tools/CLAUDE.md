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

<!-- BEGIN:xtask:autogen header -->
- Crate: coding_agent_tools
- Path: crates/tools/coding-agent-tools/
- Role: tool-lib
- Family: tools
- Integrations: mcp=true, logging=true, napi=false
<!-- END:xtask:autogen -->

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p coding_agent_tools -- --check
cargo clippy -p coding_agent_tools --all-targets -- -D warnings

# Tests
cargo test -p coding_agent_tools

# Build
cargo build -p coding_agent_tools
```
<!-- END:xtask:autogen -->
