# ENG-761: Route agentic-mcp logging to stderr to keep MCP stdout protocol-only

Linear: https://linear.app/general-wisdom/issue/ENG-761/route-agentic-mcp-logging-to-stderr-to-keep-mcp-stdout-protocol-only

Related research: `agentic-mcp-lifecycle-investigation.md`

## Goal

Make `agentic-mcp` explicitly write logs/tracing to stderr so stdout remains reserved for stdio MCP protocol messages.

This is a protocol-safety cleanup. It is not the leading explanation for the observed hangs or leaked process trees.

## Background

`agentic-mcp` is used as a stdio MCP server by `ask_agent` nested under Claude.

The investigation noted:

- `apps/agentic-mcp/src/main.rs:152` initializes tracing without explicitly forcing stderr.
- The sibling orchestrator routes tracing to stderr.

If logs ever go to stdout, MCP protocol output could be corrupted.

## Scope

- Configure `agentic-mcp` tracing/logging to write to stderr explicitly.
- Preserve existing log formatting and filtering behavior where possible.
- Add or update a focused test if the app has a suitable test harness.

## Out Of Scope

- Do not change `claudecode` lifecycle behavior here.
- Do not add `ask_agent` timeout here.
- Do not refactor MCP dispatch here.

## Starting Files

- `apps/agentic-mcp/src/main.rs`
- Sibling orchestrator logging setup can be used as a reference if needed.

## Acceptance Criteria

- Runtime logs from `agentic-mcp` do not write to stdout.
- MCP stdout remains protocol-only.
- Relevant crate-level tests or checks pass.
