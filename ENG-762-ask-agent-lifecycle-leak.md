# ENG-762: Fix claudecode process lifecycle leaks on cancelled ask_agent sessions

Linear: https://linear.app/general-wisdom/issue/ENG-762/fix-claudecode-process-lifecycle-leaks-on-cancelled-ask-agent-sessions

Related research: `agentic-mcp-lifecycle-investigation.md`

Blocks: `ENG-759`

## Goal

Fix leaked Claude and nested `agentic-mcp` processes after cancelling or replacing an active opencode model call that is inside an `ask_agent` tool call.

This is second in implementation order after `ENG-758`, because avoiding ordinary `ask_agent` hangs is the current top priority.

## Background

Confirmed lifecycle problems:

- `Session::wait` drains task handles before awaiting them, so cancellation can detach the output worker.
- The output worker owns `ProcessHandle`, so `Session::kill` becomes ineffective after worker startup.
- Claude is not spawned into its own process group, so killing only Claude may leave nested MCP children alive.
- `kill_on_drop(true)` only targets the immediate child and is not enough for the observed Claude plus nested `agentic-mcp` process tree.

Relevant paths:

- `crates/services/claudecode-rs/src/session.rs:323` - `Session::wait`
- `crates/services/claudecode-rs/src/session.rs:344` - `Session::kill`
- `crates/services/claudecode-rs/src/session.rs:410` - `Session::Drop`
- `crates/services/claudecode-rs/src/process.rs:26` - `ProcessHandle::spawn`
- `crates/services/claudecode-rs/src/process.rs:84` - `ProcessHandle::kill`

## Scope

- Make `Session::wait` cancellation-safe.
- Ensure dropping/cancelling a running `launch_and_wait` terminates Claude.
- Spawn Claude in its own Unix process group.
- Signal the process group during kill/drop cleanup so nested `agentic-mcp` is terminated too.
- Make `Session::kill` work after the output worker has started.
- Add fake-Claude tests that do not require the real Claude CLI.

## Out Of Scope

- Do not add `ask_agent` runtime timeouts here; that is `ENG-759` after cleanup is reliable.
- Do not change parser stdout/stderr behavior here if `ENG-758` is still separate.
- Do not add broad cooperative cancellation plumbing here; that is `ENG-760`.

## Starting Files

- `crates/services/claudecode-rs/src/session.rs`
- `crates/services/claudecode-rs/src/process.rs`
- Focused tests can live in `crates/services/claudecode-rs` unit tests or integration tests, using `Client::with_path()` with a fake executable.

## Test Ideas

- Fake Claude script writes its PID and PGID to temp files.
- Fake Claude spawns a long-lived child and writes the child PID and PGID.
- Cancellation/drop test: launch `client.launch_and_wait`, abort/drop it, then poll until fake Claude and child are gone.
- Explicit kill test: launch a session, wait until the output worker has started, call `Session::kill()`, then assert fake Claude and child are gone.
- Process-group test: assert Claude PGID differs from the test process PGID and fake child shares Claude PGID.

## Acceptance Criteria

- Dropping or aborting a running `launch_and_wait` future terminates the fake Claude process.
- The fake Claude child process is also terminated.
- `Session::kill` works after the output worker has taken ownership of the process.
- Claude's process group differs from the test process group.
- Fake Claude's child shares Claude's process group.
- `cargo test -p claudecode` passes.
