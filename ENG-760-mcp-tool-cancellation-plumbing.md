# ENG-760: Add cooperative cancellation plumbing for MCP tool execution

Linear: https://linear.app/general-wisdom/issue/ENG-760/add-cooperative-cancellation-plumbing-for-mcp-tool-execution

Related research: `agentic-mcp-lifecycle-investigation.md`

## Goal

Add a cooperative cancellation path for long-running MCP tool calls.

This is longer-term architecture work. It is not required for the immediate `ask_agent` hang or process-leak fixes.

## Background

Current MCP dispatch awaits tool futures directly and creates a default `ToolContext` with no cancellation token:

- `crates/agentic-tools/mcp/src/server.rs:202` - `RegistryServer::call_tool`
- `crates/agentic-tools/mcp/src/server.rs:220` - `dispatch_json_formatted(...).await`
- `crates/agentic-tools/core/src/context.rs:10` - `ToolContext`

This means long-running tools cannot cooperatively observe client cancellation. `claudecode` cleanup can kill subprocesses on drop after `ENG-762`, but a broader cancellation model would let tool implementations stop cleanly.

## Scope

- Investigate how `rmcp` exposes cancellation or request disconnect signals.
- Add cancellation support to `ToolContext` if feasible.
- Thread cancellation through MCP dispatch.
- Update long-running tool implementations to observe cancellation where appropriate.
- Keep behavior backward-compatible for tools that ignore cancellation.

## Out Of Scope

- Do not use this as the first fix for leaked Claude processes; `ENG-762` should handle subprocess cleanup even without cooperative cancellation.
- Do not add `ask_agent` runtime timeout here; that is `ENG-759`.

## Starting Files

- `crates/agentic-tools/mcp/src/server.rs`
- `crates/agentic-tools/core/src/context.rs`
- Tool implementations that perform long-running work, if adding focused cancellation tests

## Acceptance Criteria

- MCP tool calls can receive a cancellation signal through `ToolContext`.
- At least one long-running tool path has a focused cancellation test.
- Existing tool dispatch behavior is unchanged when no cancellation occurs.
- Relevant crate-level tests pass.
