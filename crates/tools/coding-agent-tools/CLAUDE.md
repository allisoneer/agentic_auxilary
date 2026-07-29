# CLAUDE.md - coding_agent_tools

## Purpose
CLI/MCP discovery tools plus hermetic Claude subagent orchestration.

## Quick Commands
```bash
just check        # Run formatting and clippy checks
just test         # Run tests
just build        # Build the project
just fmt          # Format code
```

## Architecture
- `src/lib.rs` - Tool implementations, sandboxed path routing, and the internal `SessionOutcome` runner
- `src/agent/config.rs` - strict model aliases and exact eight-cell eager MCP matrix
- `src/agent/guidance.rs` - bounded Git-index-tracked repository guidance
- `src/agent/evidence.rs` - raw init/tool-call/tool-result success gate
- `src/agent/live_tests.rs` - ignored Claude 2.1.220 nonce matrix and eager/deferred controls
- `src/types.rs` - Depth, Show, LsOutput, LsEntry types
- `src/paths.rs` - Path normalization utilities
- `src/walker.rs` - Directory traversal with ignore/globset
- `src/pagination.rs` - Implicit pagination state for MCP

## Key Design Decisions
- `ignore` crate with `parents(false)` for gitignore-aware traversal
- `globset` for custom ignore patterns (not `add_ignore()`)
- Pagination state in struct; CLI creates fresh instance (no pagination), MCP reuses Arc-wrapped instance
- McpFormatter for token-efficient text output
- Spawned agents use no Claude built-ins or inherited setting sources. One eager strict `agentic-mcp --nested-profile ... --allow ...` server supplies the exact cell surface.
- Nested CLI roots are canonical and read-only. Web cells have no local tools; no cell receives edit or apply-patch.
- Todo state is complete-list replacement scoped to one nested MCP process.
- Public `AgentOutput` remains text, but production accepts it only after structured transcript evidence succeeds.
- Evidence requires assistant `tool_use` followed by a matching successful user `tool_result`; wrong roles, reversed order, errors, and todo-only activity fail closed. Live nonces must originate in a paired expected tool result and remain absent from both the query and complete composed system prompt; tracked-guidance sentinels are distinct values.
- Nested diagnostics rely on SDK value-based redaction so known configured credentials cannot persist in generic stdout, stderr, transcript, or tool-result strings.

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
