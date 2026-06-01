# agentic-mcp ask_agent Lifecycle Investigation

Date: 2026-04-30

Purpose: shared handoff file for combining research from multiple sessions before selecting and implementing the smallest safe lifecycle fix.

## Original Observation

Root opencode session spawned parent `agentic-mcp`, which spawned Claude for `ask_agent`. Claude then spawned a nested `agentic-mcp` from the MCP config.

Observed leaked subprocesses:

```text
opencode PID 1007351
agentic-mcp PID 1007411
claude PID 1009134
nested agentic-mcp PID 1009159
```

After abort/resubmit, new Claude/nested `agentic-mcp` pairs appeared while old pairs stayed alive as descendants of the parent `agentic-mcp`.

Expected behavior: aborting/replacing/completing the parent tool call should terminate and reap the old Claude process and its nested MCP child tree.

Actual behavior: old Claude and nested `agentic-mcp` remained alive with idle sockets and no disk IO.

## Code Path

`ask_agent` lives in `crates/tools/coding-agent-tools/src/lib.rs`.

Key flow:

```text
CodingAgentTools::ask_agent
  -> build MCP config with nested agentic-mcp allowlist
  -> validate MCP config
  -> Client::new()
  -> client.launch_and_wait(config).await
```

Relevant references:

```text
crates/tools/coding-agent-tools/src/lib.rs:260 ask_agent entry point
crates/tools/coding-agent-tools/src/lib.rs:309 build nested MCP config
crates/tools/coding-agent-tools/src/lib.rs:313 ensure_valid_mcp_config
crates/tools/coding-agent-tools/src/lib.rs:381 client.launch_and_wait(config).await
crates/tools/coding-agent-tools/src/agent/config.rs:139 build_mcp_config
crates/tools/coding-agent-tools/src/agent/config.rs:153 --allow nested agentic-mcp args
crates/tools/coding-agent-tools/src/agent/config.rs:160 nested server name agentic-mcp
```

Claude process launch is in `crates/services/claudecode-rs`.

Relevant references:

```text
crates/services/claudecode-rs/src/client.rs:41 Client::launch
crates/services/claudecode-rs/src/client.rs:71 Client::launch_and_wait
crates/services/claudecode-rs/src/process.rs:26 ProcessHandle::spawn
crates/services/claudecode-rs/src/process.rs:32 Command::new
crates/services/claudecode-rs/src/process.rs:34 stdin null
crates/services/claudecode-rs/src/process.rs:35 stdout piped
crates/services/claudecode-rs/src/process.rs:36 stderr piped
crates/services/claudecode-rs/src/process.rs:37 kill_on_drop(true)
crates/services/claudecode-rs/src/session.rs:48 Session::new
crates/services/claudecode-rs/src/session.rs:89 Session::start_tasks
crates/services/claudecode-rs/src/session.rs:284 handle_text
crates/services/claudecode-rs/src/session.rs:323 Session::wait
crates/services/claudecode-rs/src/session.rs:344 Session::kill
crates/services/claudecode-rs/src/session.rs:410 Session::Drop
```

MCP server dispatch itself is direct awaiting with no cancellation context threaded into tools:

```text
crates/agentic-tools/mcp/src/server.rs:202 RegistryServer::call_tool
crates/agentic-tools/mcp/src/server.rs:220 dispatch_json_formatted(...).await
crates/agentic-tools/core/src/context.rs:10 ToolContext currently has no cancellation token
```

## Primary Suspected Bug

`Session::wait` drains task handles before awaiting them:

```text
crates/services/claudecode-rs/src/session.rs:323
```

Current behavior:

```text
for task in self.tasks.drain(..) {
    let _ = task.await;
}
```

If the parent MCP tool future is cancelled while awaiting one of those drained handles, the `Session` is dropped, but `Session::Drop` no longer has that handle in `self.tasks`. It therefore cannot abort the worker task.

The worker task has already taken ownership of the `ProcessHandle`:

```text
crates/services/claudecode-rs/src/session.rs:288 handle_text locks process
crates/services/claudecode-rs/src/session.rs:289-293 takes ProcessHandle out of Option
```

Result: the worker task survives cancellation and keeps the Claude process alive. Claude keeps its nested MCP `agentic-mcp` alive.

This matches the observed process tree: old Claude processes remain direct children of the parent `agentic-mcp` after resubmission.

## Secondary Lifecycle Issues

`Session::kill()` becomes ineffective after the background worker has taken the process handle:

```text
crates/services/claudecode-rs/src/session.rs:346
```

Because `self.process` is already `None`, kill cannot reach the child. This also affects any explicit cancellation path that calls `Session::kill()` after launch.

No process groups are used in `ProcessHandle::spawn`. `kill_on_drop(true)` only targets the immediate child. It does not reliably kill Claude's nested `agentic-mcp` server or further descendants.

Live diagnostic command showed all involved processes sharing the root opencode process group rather than each Claude run getting an isolated group:

```text
ps -p 1007411,1009134,1009159,1012292,1012361,1012320,1012360 -o pid,ppid,pgid,sid,stat,etimes,wchan,args
```

Representative result:

```text
PID      PPID     PGID     SID
1007411  1007351  1007351  480842  agentic-mcp
1009134  1007411  1007351  480842  claude ...
1009159  1009134  1007351  480842  agentic-mcp --allow ...
```

Killing by process group is unsafe until Claude gets its own group, because otherwise killing Claude's PGID would target the root opencode group.

## Pipe Handling Concern

Text and single-JSON parsers read stdout to EOF before reading stderr:

```text
crates/services/claudecode-rs/src/stream.rs:82 SingleJsonParser::parse reads stdout then stderr
crates/services/claudecode-rs/src/stream.rs:122 TextParser::parse reads stdout then stderr
```

If Claude or a wrapper writes enough stderr to fill the pipe while stdout remains open, the child can block forever and the parent can wait forever on stdout EOF. This is a real pipe deadlock pattern.

Streaming JSON reads stderr in a background task, so this specific issue is mainly text/json output mode. `ask_agent` uses `OutputFormat::Text`.

## Timeout Concern

MCP validation has timeouts:

```text
crates/services/claudecode-rs/src/mcp/validate.rs:47 default validation timeouts
crates/services/claudecode-rs/src/mcp/validate.rs:633 handshake timeout
crates/services/claudecode-rs/src/mcp/validate.rs:664 tools/list timeout
crates/services/claudecode-rs/src/mcp/validate.rs:691 overall timeout
```

The actual Claude `ask_agent` run has no timeout around:

```text
client.launch_and_wait(config).await
```

If Claude stalls after MCP startup and initial tool use, it can remain indefinitely.

## Task Map Cleanup

No internal `ask_agent` task map was found in `coding-agent-tools`, `agentic-tools-mcp`, or `claudecode-rs`.

The relevant lifecycle collection is only `Session.tasks: Vec<JoinHandle<()>>` in `claudecode-rs/src/session.rs`. The cancellation bug is tied to removal from that vector before completion.

## Minimal Fix Candidate

Implement in `crates/services/claudecode-rs` first, because this is where process ownership and cancellation safety belong.

Candidate changes:

1. Make `Session::wait` cancellation-safe by not removing a `JoinHandle` from `self.tasks` until it has completed.
2. Ensure dropping `Session` while `wait()` is awaiting still aborts the worker task.
3. Put Claude in its own process group when spawning.
4. Store enough process identity in `ProcessHandle` to signal the process group.
5. Make `ProcessHandle::kill`, `Session::interrupt`, and process cleanup signal the process group, not only the direct Claude PID.
6. Read stdout and stderr concurrently in `TextParser` and `SingleJsonParser`.
7. Consider adding a conservative `ask_agent` timeout after cancellation cleanup is reliable.

The smallest first patch may be only item 1, but it will not guarantee nested MCP children are killed. For the observed Claude plus nested `agentic-mcp` tree, item 1 plus process-group cleanup is the safer minimal production fix.

## Test Ideas

Feasible tests without real Claude:

1. Add test-only construction path or use `Client::with_path` with a fake executable script.
2. Fake Claude script spawns a child process and sleeps.
3. Launch session, start `launch_and_wait`, abort/drop the future, then assert fake Claude and child are gone.
4. Fake Claude script writes more than a pipe buffer to stderr while keeping stdout open; assert text parsing does not hang after concurrent read fix.
5. Fake Claude script records PID/PGID into temp files; assert Claude PGID differs from parent process PGID.
6. Test `Session::kill()` after worker task has taken `ProcessHandle`; current code likely fails, fixed code should terminate process group.

Test constraints:

```text
ProcessHandle fields are private.
Session::new requires a ProcessHandle.
Client::with_path can run a fake claude binary.
Temp scripts should live under ${TMPDIR:-/tmp}/agents or test temp dirs.
```

## Open Questions For Other Research Session

Please append your findings below rather than editing the summary above.

Areas to verify:

1. Does `rmcp` cancel/drop the `call_tool` future when opencode aborts or resubmits a prompt?
2. Does opencode itself send any MCP cancellation notification that `RegistryServer::call_tool` could observe?
3. Is there an existing Tokio child process-group API available in the pinned Tokio version, or should this use Unix `pre_exec` with `libc::setpgid(0, 0)`?
4. What is the most correct process-group cleanup sequence: SIGTERM group, short grace period, SIGKILL group, then wait/reap direct child?
5. Does Claude install signal handlers or spawn nested MCP servers in a way that requires killing the group instead of relying on Claude to clean up?
6. Are there existing repo patterns for subprocess group cleanup in `opencode-rs`, `review-tools`, or orchestrator code that should be reused?
7. Would adding an `ask_agent` timeout break legitimate long-running analyzer calls, and should timeout be config-driven under `SubagentsConfig`?

## Append Section For Other Session

Other session findings should start here.

### Other Session Findings

TBD

Independent session notes from 2026-04-30:

- Verified repo identity before source/live diagnostics: cwd and git root were `/home/altendky/repos/agentic_auxilary`; remotes were `origin git@github.com:altendky/agentic_auxilary.git` and `upstream git@github.com:allisoneer/agentic_auxilary`; branch was `main...upstream/main`. The worktree already had deleted `opencode.json`, which was not touched.
- Live process snapshot still showed the leaked tree alive under opencode PID `1007351`: parent `agentic-mcp` PID `1007411`, Claude PIDs `1009134`, `1012292`, `1012320`, and nested MCP PIDs `1009159`, `1012361`, `1012360`.
- Live `ps` showed all relevant processes in the root opencode process group rather than a per-Claude group: `PGID=1007351`, `SID=480842` for `opencode`, parent `agentic-mcp`, each `claude --print`, and each nested `agentic-mcp`. This supports the existing warning that group-killing is unsafe until Claude is spawned into its own group.
- Live DB query against `ses_220365e5effeV1lFH6Kk4Xmc1w` still showed two `tools_ask_agent` parts marked `running`: `call_LhKcppYENWGnKDHCDoW8wXO3` and `call_ib5XMgvDRuMGcErquUeMPwIs`. The resubmitted `call_Gail4FGkbfilgqgIs2EwNB4F` was `completed`, confirming the failure is not universal.
- `pidstat -d` on the live Claude/nested MCP PIDs showed 0 disk IO. `pidstat -t` showed low but nonzero CPU in Claude threads and effectively none in nested `agentic-mcp`. This fits an event-loop/protocol wait or lost cancellation more than active file scanning.
- `tools_ask_agent` registration path: `crates/tools/coding-agent-tools/src/tools.rs:95` defines `AskAgentInput`, `tools.rs:108` defines `AskAgentTool`, `tools.rs:120` implements `Tool`, and `tools.rs:153` delegates to `CodingAgentTools::ask_agent`.
- Main launch path: `crates/tools/coding-agent-tools/src/lib.rs:259` starts `CodingAgentTools::ask_agent`; `lib.rs:308` builds the MCP config; `lib.rs:311` validates it; `lib.rs:333` builds `SessionConfig`; `lib.rs:381` awaits `client.launch_and_wait(config)` with no surrounding timeout.
- Claude CLI construction path: `crates/services/claudecode-rs/src/client.rs:41` launches sessions, `client.rs:71` implements `launch_and_wait`, `client.rs:101` adds `--print`, `client.rs:122` adds `--mcp-config`, `client.rs:129` adds `--strict-mcp-config`, and `client.rs:241` appends `--` plus the query.
- Nested MCP config path: `crates/tools/coding-agent-tools/src/agent/config.rs:136` defines `build_mcp_config`, `config.rs:153` builds args `--allow <allowlist> --suppress-search-reminder`, and `config.rs:159` inserts a stdio MCP server named `agentic-mcp`.
- Spawn details: `crates/services/claudecode-rs/src/process.rs:32` creates `tokio::process::Command`, `process.rs:34` uses null stdin, `process.rs:35` and `process.rs:36` pipe stdout/stderr, and `process.rs:37` enables `kill_on_drop(true)`. No `process_group`, `setsid`, `setpgid`, `killpg`, or Unix `CommandExt` usage was found in the repo.
- Ownership detail: `crates/services/claudecode-rs/src/session.rs:66` stores `ProcessHandle` in `Arc<Mutex<Option<ProcessHandle>>>`, but `handle_text` takes it at `session.rs:288-293`, so the `Session` object quickly loses direct access to the child.
- Explicit `Session::kill` likely cannot work once the output task has started: `crates/services/claudecode-rs/src/session.rs:345` only kills a process still present in `self.process`; after `handle_text` takes it, the option is `None`.
- `Session::is_running` is misleading for the same reason: `crates/services/claudecode-rs/src/session.rs:389` returns false when `self.process` is `None`, even if the background task still owns and waits on the live child.
- Text and single-JSON pipe draining are sequential: `crates/services/claudecode-rs/src/stream.rs:78` and `stream.rs:118` read stdout to EOF before stderr. Streaming JSON avoids this specific pattern by spawning a stderr reader in `crates/services/claudecode-rs/src/session.rs:167`.
- MCP validation already has bounded timeouts and drains stderr: `crates/services/claudecode-rs/src/mcp/validate.rs:568` sets `kill_on_drop(true)`, `validate.rs:590` starts a stderr reader, `validate.rs:631` wraps handshake timeout, `validate.rs:664` wraps tools/list timeout, and `validate.rs:691` wraps the overall timeout.
- `apps/agentic-mcp/src/main.rs:152` calls `tracing_subscriber::fmt::init()` without explicitly forcing stderr. The sibling orchestrator routes tracing to stderr. If `tracing_subscriber` ever writes to stdout here, MCP stdout could be corrupted. This is not my primary hypothesis for the live stuck processes because the nested MCP validations and some ask-agent calls complete, but it is a protocol-safety cleanup candidate.

### Other Session Proposed Fixes

TBD

Independent session proposed fixes from 2026-04-30:

- First fix the `Session::wait` cancellation-safety bug. The current `for task in self.tasks.drain(..) { let _ = task.await; }` pattern removes the `JoinHandle` before the await completes. If the parent future is cancelled while awaiting that handle, the handle is dropped and the task is detached, so `Session::Drop` cannot abort it. Keep abort handles in `Session`, or remove each task only after it has completed.
- Make Claude process cleanup group-aware on Unix. Spawn each Claude child into its own process group with `CommandExt::pre_exec(|| { libc::setpgid(0, 0); Ok(()) })` or Tokio's process-group API if available in the resolved Tokio version. Then terminate `-pgid`, not only the direct child PID.
- Use a two-stage cleanup sequence for group termination: send SIGTERM to the Claude process group, wait a short grace period, then send SIGKILL to the process group, then wait/reap the direct child. Avoid killing by group until Claude has its own PGID.
- Make `Session::kill` effective after the worker task takes `ProcessHandle`. One small design is to keep a shared process-control handle containing child PID/PGID and an abort handle, while the output task owns the `Child` for waiting and pipe reads. `Session::kill` can then signal the group and abort/join the worker even when `self.process` is empty.
- Consider replacing the background-task design for text/json with a single owned future if possible. `launch_and_wait` could own the process directly and parse/wait inline, avoiding the detached-task cancellation hazard. This may be a larger refactor than preserving the current `Session` API.
- Drain stdout and stderr concurrently in `TextParser` and `SingleJsonParser`, for example with `tokio::try_join!` over both `read_to_string` calls. This is independent of process-group cleanup and protects `ask_agent` text mode from stderr pipe backpressure.
- Add a configurable timeout around the actual Claude ask-agent run after cancellation cleanup is reliable. A conservative first implementation could live in `CodingAgentTools::ask_agent` around `client.launch_and_wait(config)`, but the timeout value should likely be configurable through subagent config to avoid breaking legitimate analyzer calls.
- Route `agentic-mcp` tracing explicitly to stderr in `apps/agentic-mcp/src/main.rs`, matching the orchestrator's MCP stdout hygiene. This is a small safety fix but not sufficient for lifecycle cleanup.
- Longer term, add cancellation plumbing to `ToolContext` and `RegistryServer::call_tool` so MCP cancellation notifications can be propagated into long-running tool implementations, if `rmcp` exposes them.

### Other Session Test Notes

TBD

Independent session test notes from 2026-04-30:

- Add focused tests under `crates/services/claudecode-rs`, preferably not requiring real Claude. Use `Client::with_path()` with a temp fake `claude` script.
- Fake Claude script for lifecycle tests: write its PID and PGID to temp files, spawn a long-lived child such as `sleep 60`, write the child PID/PGID, then keep stdout open or sleep so the Rust session remains active.
- Cancellation test: launch a text-mode session with fake Claude, start `session.wait()` or `client.launch_and_wait`, abort/drop that future, then poll until both the fake Claude parent and spawned child are gone. This should fail on current code if the output task is detached.
- Explicit kill test: launch fake Claude, wait until the text handler has had time to take the `ProcessHandle`, call `Session::kill()`, then assert fake Claude and child are gone. This directly targets `session.rs:345` becoming ineffective after `handle_text` takes the process.
- Process-group test: fake Claude records PGID; assert the Claude PGID differs from the test process PGID and that the fake child shares Claude's PGID. This verifies the process-group setup before relying on `killpg` semantics.
- Pipe-deadlock test: fake Claude writes more than a pipe buffer to stderr while keeping stdout open. With the current sequential parser, text/json parsing can hang; after concurrent reads, the parser should make progress or return a controlled error under a test timeout.
- MCP validation timeout cleanup test: add a sibling to `test_overall_timeout` in `crates/services/claudecode-rs/src/mcp/validate.rs` using a shell command that spawns a child and sleeps, then assert timeout cleanup kills the subprocess tree once group cleanup is implemented for validation too.
- Existing tests found: `crates/tools/coding-agent-tools/tests/ask_agent_integration.rs` has ignored live-Claude ask-agent coverage only; `crates/services/claudecode-rs/tests/integration.rs:40` has `test_session_cancellation`, but it requires real Claude and only asserts `session.kill().await.is_ok()`; `crates/services/claudecode-rs/src/mcp/validate.rs:946` has `test_overall_timeout`, but it only asserts the error type and not process cleanup.
- Verification command after implementation should start with `cargo test -p claudecode` or the repo recipe `just crate-test claudecode`. If process-group code is Unix-only, gate tests accordingly.

### Conflicts Or Disagreements

TBD

Independent session conflicts or disagreements from 2026-04-30:

- I agree with the primary suspected bug in the existing summary. The `Session::wait` drain-before-await pattern is a credible explanation for leaked direct Claude children because dropping a `JoinHandle` detaches the task that owns `ProcessHandle`.
- I would not ship only the `Session::wait` fix as the production fix for the observed tree. It may stop detached output tasks, but it still relies on direct-child `kill_on_drop`; Claude's nested `agentic-mcp` child is not guaranteed to die without process-group isolation and group signaling.
- I rank the sequential stdout/stderr parser as a real bug but not the best match for the live diagnostics. The live processes were idle in `ep_poll`/`futex_do_wait` with no disk IO and no obvious write-block evidence from non-invasive checks. It should still be fixed because `ask_agent` uses text mode.
- I do not think opencode DB cleanup is the root bug. The DB remains `running` because the MCP tool future is still waiting or has been detached without producing a result. Once `ask_agent` returns an error on timeout/cancellation, opencode should be able to mark the part non-running.
- `ENABLE_LSP_TOOLS` being unset was verified in the live process tree and looks unrelated to this lifecycle leak.
- The fact that `call_Gail4FGkbfilgqgIs2EwNB4F` completed while the two later calls hung suggests the nested MCP config and basic pipe wiring can work. The bug is likely triggered by cancellation/replacement or a Claude/MCP protocol stall without a timeout, not by every `ask_agent` launch.

## Processor Session Goal

Use this file as the single source of research truth. Re-read the code before editing. Identify the smallest safe fix, implement it, and run focused verification.

Suggested processor acceptance criteria:

1. Cancelling/dropping a running `ask_agent` future terminates Claude.
2. Claude's nested `agentic-mcp` child also terminates.
3. Explicit `Session::kill()` works even after the worker task has started reading output.
4. Text/json output parsing cannot deadlock on stderr pipe backpressure.
5. Existing `claudecode` tests pass.
6. Focused new lifecycle tests pass without requiring real Claude or live network access.
