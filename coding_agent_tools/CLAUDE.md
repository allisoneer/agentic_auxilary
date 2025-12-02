# CLAUDE.md - coding_agent_tools

## Purpose
CLI + MCP tools for coding assistants. Currently implements `ls` tool for gitignore-aware directory listing.

## Quick Commands
```bash
make all          # Check, test, and build
make check        # Run formatting and clippy checks
make test         # Run tests
make build        # Build the project
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
