# ENG-759: Add configurable ask_agent runtime timeout after cleanup is reliable

Linear: https://linear.app/general-wisdom/issue/ENG-759/add-configurable-ask-agent-runtime-timeout-after-cleanup-is-reliable

Related research: `agentic-mcp-lifecycle-investigation.md`

Blocked by: `ENG-762`

## Goal

Add a configurable timeout around the main `ask_agent` Claude run, after `claudecode` process cleanup is reliable.

Timeouts are mitigation, not a root-cause fix. They bound hangs and turn them into controlled errors.

## Background

The main `ask_agent` path awaits Claude without a runtime timeout:

- `crates/tools/coding-agent-tools/src/lib.rs:381` - `client.launch_and_wait(config).await`

MCP validation already has bounded timeouts, but the actual Claude run does not.

Do not implement this before `ENG-762`; otherwise a timeout may cancel the future while leaving Claude or nested `agentic-mcp` processes alive.

## Scope

- Add a configurable runtime timeout for `ask_agent`.
- Prefer configuration under subagent settings rather than a hard-coded behavior change.
- Surface timeout failures clearly in the tool response and logs.
- Rely on fixed `claudecode` cleanup so timeout cancellation terminates Claude and nested MCP children.

## Out Of Scope

- Do not use timeout as a substitute for fixing parser deadlocks or process lifecycle leaks.
- Do not add timeout until `ENG-762` is complete.

## Starting Files

- `crates/tools/coding-agent-tools/src/lib.rs`
- `crates/tools/coding-agent-tools/src/agent/config.rs` or related subagent config files, if adding configuration
- `crates/services/claudecode-rs` tests may be relevant only to verify cleanup behavior already covered by `ENG-762`

## Acceptance Criteria

- `ask_agent` returns a clear timeout error when the configured limit is exceeded.
- Timeout cancellation does not leave Claude or nested MCP children running.
- Legitimate long-running analyzer calls can opt into a longer timeout.
- Relevant `coding_agent_tools` and `claudecode` tests pass.
