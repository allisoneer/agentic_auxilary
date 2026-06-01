# ask_agent Lifecycle Debug Plan

Date: 2026-04-30

Goal: prove the root cause of stuck or leaked `ask_agent` subprocess trees before shipping a fix. The observed shape is:

```text
opencode
  -> agentic-mcp
    -> claude --print
      -> nested agentic-mcp --allow ...
```

Some `tools_ask_agent` calls remain `running` in the opencode DB while their Claude and nested MCP subprocesses stay alive and idle.

## Repository Facts

- Repo: `/home/altendky/repos/agentic_auxilary`
- Remotes: `origin git@github.com:altendky/agentic_auxilary.git`, `upstream git@github.com:allisoneer/agentic_auxilary`
- Language: Rust workspace, edition 2024
- Package manager: Cargo
- Test runner: `cargo nextest` through `just test` and `just crate-test <crate>`
- Relevant crates: `apps/agentic-mcp`, `crates/tools/coding-agent-tools`, `crates/services/claudecode-rs`, `crates/agentic-tools/mcp`, `crates/agentic-tools/core`

## Source Map

| Area | Source | Relevance |
| --- | --- | --- |
| Parent MCP binary | `apps/agentic-mcp/src/main.rs:150-213` | Serves MCP over stdio and waits for service shutdown. |
| MCP dispatch | `crates/agentic-tools/mcp/src/server.rs:202-223` | Directly awaits tool future. No timeout or cancellation token. |
| Tool context | `crates/agentic-tools/core/src/context.rs:3-13` | No cancellation token yet. |
| Registry dispatch | `crates/agentic-tools/core/src/registry.rs:133-145`, `279-309` | Awaits erased tool future directly. |
| Tool wrapper | `crates/tools/coding-agent-tools/src/tools.rs:95-164` | Defines `ask_agent` input and delegates to `CodingAgentTools::ask_agent`. |
| Main implementation | `crates/tools/coding-agent-tools/src/lib.rs:259-443` | Builds MCP config, validates it, launches Claude, logs result. |
| Nested MCP config | `crates/tools/coding-agent-tools/src/agent/config.rs:42-167` | Builds `agentic-mcp --allow <tools> --suppress-search-reminder`. |
| Subagent config | `crates/infra/agentic-config/src/types.rs:51-70` | Only model fields exist; no runtime timeout config. |
| Claude args | `crates/services/claudecode-rs/src/client.rs:41-74`, `94-252` | Builds and runs `claude --print ... --mcp-config <temp> ... -- <query>`. |
| Process spawn | `crates/services/claudecode-rs/src/process.rs:26-87` | Uses null stdin, piped stdout/stderr, `kill_on_drop(true)`, no process group. |
| Session tasks | `crates/services/claudecode-rs/src/session.rs:89-140`, `284-327`, `344-417` | Worker task takes `ProcessHandle`; `wait` drains task handles before awaiting. |
| Pipe parsing | `crates/services/claudecode-rs/src/stream.rs:78-84`, `118-124` | Text/JSON parsers read stdout fully before stderr. |
| MCP validation | `crates/services/claudecode-rs/src/mcp/validate.rs:566-700` | Validation has timeouts and stderr drain; main Claude run does not. |
| Tool logging | `crates/tools/coding-agent-tools/src/logging.rs:29-107` | JSONL records use a call id, but cancellation/drop is not recorded. |

## Lifecycle

```text
RegistryServer::call_tool("ask_agent")
  -> ToolRegistry::dispatch_json_formatted
    -> AskAgentTool::call
      -> CodingAgentTools::ask_agent
        -> enabled_tools_for / compose_prompt / build_mcp_config
        -> ensure_valid_mcp_config
          -> temporary validation process with bounded timeout
        -> SessionConfig::builder(query)
          -> OutputFormat::Text
          -> PermissionMode::DontAsk
          -> strict MCP config
        -> Client::new
        -> Client::launch_and_wait
          -> ProcessHandle::spawn("claude", args)
            -> stdin null
            -> stdout piped
            -> stderr piped
            -> kill_on_drop(true)
          -> Session::new
            -> spawn output worker
          -> Session::wait
            -> await output worker
              -> worker owns ProcessHandle
              -> TextParser reads stdout then stderr
              -> wait for Claude exit
```

Cancellation-sensitive path:

```text
Session::wait drains a JoinHandle out of self.tasks before awaiting it.
If the parent future is dropped while awaiting that handle:
  Session::Drop cannot abort that worker.
  Dropping the JoinHandle detaches the task.
  The detached task still owns ProcessHandle.
  Claude remains alive and can keep nested agentic-mcp alive.
```

## Ranked Hypotheses

1. Unbounded Claude or nested MCP protocol stall. `CodingAgentTools::ask_agent` awaits `client.launch_and_wait(config)` without a runtime timeout at `crates/tools/coding-agent-tools/src/lib.rs:381`.
2. Cancelled or replaced request detaches the `claudecode` worker. `Session::wait` drains task handles before awaiting at `crates/services/claudecode-rs/src/session.rs:323-327`; `handle_text` already took the `ProcessHandle` at `session.rs:288-293`.
3. No process-group isolation. `ProcessHandle::spawn` uses `kill_on_drop(true)` but does not isolate Claude into its own PGID, so nested `agentic-mcp` children are not guaranteed to die.
4. `Session::kill` becomes ineffective after worker startup. It only kills a process still present in `self.process`, but the worker usually takes it first.
5. Sequential stdout/stderr reads can deadlock. `TextParser` and `SingleJsonParser` read stdout before stderr, which can hang if stderr fills while stdout remains open.
6. MCP stdout protocol contamination is possible. `agentic-mcp` does not explicitly force tracing to stderr, though startup logs use `eprintln!` and this is not the leading explanation.
7. Direct concurrent ask-agent state interference is unlikely. Each call has its own `Client`, `Session`, temp config, and worker; the concurrency hazard is shared PGID.

## Automated Reproduction

Add tests before fixes so the current failure modes are proven.

### Primary test target

Add `crates/services/claudecode-rs/tests/process_lifecycle.rs`.

Use `Client::with_path(fake_claude)` to avoid real Claude and avoid global env changes.

Test cases:

| Test | Fake behavior | Expected current result | Expected fixed result |
| --- | --- | --- | --- |
| `drop_launch_and_wait_kills_process_tree` | Fake Claude writes PID/PGID, spawns child/grandchild, sleeps forever. | Process tree survives after abort/drop. | Direct child and descendants terminate and are reaped. |
| `session_kill_after_worker_starts_kills_process_tree` | Fake Claude sleeps after writing PID files. | `Session::kill()` can return success while child survives. | Kill terminates process group and `wait` does not hang. |
| `claude_runs_in_own_process_group` | Fake Claude records PGID and child PGID. | Claude PGID matches test/opencode PGID. | Claude PGID differs; descendants share Claude PGID. |
| `process_reaped_no_zombie` | Fake Claude exits or is killed. | Possible persistent zombie or detached child. | Direct child is reaped; no persistent `Z` state. |

Assertions:

- Poll `kill(pid, 0)` until `ESRCH` for Claude, child, and grandchild.
- On Linux, check `/proc/<pid>/stat` and fail on persistent zombie state `Z`.
- Confirm Claude PGID differs from the test process PGID.
- Confirm fake child and grandchild PGID equals Claude PGID after group isolation is implemented.

### Pipe-deadlock test target

Add focused tests in `crates/services/claudecode-rs/src/stream.rs`.

Use `tokio::io::duplex` or fake async readers to model stderr receiving more than a pipe buffer while stdout stays open.

Test cases:

- `text_parser_drains_stdout_and_stderr_concurrently`
- `single_json_parser_drains_stdout_and_stderr_concurrently`

Wrap parser execution in a short `tokio::time::timeout` so regressions fail instead of hanging.

### ask_agent integration target

Add `crates/tools/coding-agent-tools/tests/ask_agent_fake_claude.rs`.

Use `#[serial]`, `CLAUDE_PATH=/tmp/.../claude`, and a temp directory prepended to `PATH` containing fake `agentic-mcp`.

Fake nested `agentic-mcp` behavior:

- Minimal stdio JSON-RPC server.
- Reply to `initialize`.
- Reply to `tools/list` with the expected tool names: `cli_ls`, `cli_grep`, `cli_glob`, `web_search`, `web_fetch`, and thoughts tools as needed.
- Ignore notifications.

Fake Claude variants:

- `ok_text`: print `fake claude response`, exit 0.
- `partial_stdout_then_hang`: write partial stdout, keep stdout open, sleep.
- `mcp_wait`: simulate waiting forever on MCP response.
- `stderr_open_idle`: keep stderr open without writing.
- `grandchild_ignores_parent_exit`: spawn descendant that survives ordinary parent exit.

Assertions:

- Success path returns text and leaves no subprocesses.
- Cancel/timeout/drop paths terminate direct Claude and nested/grandchild processes.
- Tool call future returns success or controlled error; it does not hang indefinitely.

## Diagnostics To Add

Gate noisy lifecycle tracing with `AGENTIC_ASK_AGENT_LIFECYCLE=1` plus normal `RUST_LOG`. Do not log full prompts, full env values, API keys, or full stderr by default.

| Source | Log fields |
| --- | --- |
| `CodingAgentTools::ask_agent` | `call_id`, `agent_type`, `location`, `model`, `query_len`, enabled tool count, built-in tools, MCP allowlist, validation duration, Claude duration. |
| `agent/config.rs` | Nested MCP server name, command, sanitized args, allowlist. |
| `Client::launch` | Claude path, sanitized args, output format, working dir, strict MCP flag, MCP temp path. |
| `ProcessHandle::spawn` | Command, args count, cwd, env key names only, child PID, PGID, SID. |
| `ProcessHandle::wait` | PID, PGID, exit status, elapsed time. |
| `ProcessHandle::kill` | PID, PGID, signal sent, result. |
| `Session::start_tasks` | Session id, output format, worker start. |
| `Session::handle_text` | Process handle taken, stdout/stderr drain start/end, byte counts, parser result, wait start/end. |
| `Session::wait` | Task count, await start/end. |
| `Session::Drop` | Session dropped, remaining task count, aborts issued, process present or already taken. |
| `TextParser` / `SingleJsonParser` | stdout bytes, stderr bytes, concurrent drain completion. |
| `mcp/validate.rs` | Validation child command, args, handshake/tools timing, timeout kind, stderr tail length. |
| `RegistryServer::call_tool` | Tool name, dispatch start/end, elapsed time, success/error. |
| `agentic-mcp` startup | PID, PGID, SID, allowlist, tool count, inherited ask-agent call id, output mode. |

Correlation:

- Use `ToolLogCtx` call id from `log_ctx.timer.call_id`.
- Pass `AGENTIC_ASK_AGENT_CALL_ID`, `AGENTIC_ASK_AGENT_TYPE`, and `AGENTIC_ASK_AGENT_LOCATION` into Claude via `SessionConfig::env_var`.
- Pass the same call id into nested MCP config with `MCPServer::stdio_with_env` if needed.
- Put lifecycle rollup data in the existing JSONL `summary`; keep high-cardinality step events in tracing.

## Debug Build Strategy

Build local debug binaries:

```bash
cargo build -p agentic-mcp
```

Run a live debug session with the debug binary first in `PATH`:

```bash
PATH=/home/altendky/repos/agentic_auxilary/target/debug:$PATH \
RUST_LOG=agentic_mcp=debug,coding_agent_tools=debug,claudecode=debug,agentic_tools_mcp=debug \
AGENTIC_ASK_AGENT_LIFECYCLE=1 \
RUST_BACKTRACE=1 \
opencode
```

If opencode uses an absolute `agentic-mcp` path, point that debug session at `target/debug/agentic-mcp`. The nested MCP config currently uses command name `agentic-mcp`, so `PATH` controls nested resolution.

## Live Diagnostics

Repeat this on stuck trees:

```bash
pstree -ap <opencode-pid>
ps -p <claude>,<nested-agentic-mcp> -o pid,ppid,pgid,sid,stat,etimes,time,wchan:32,args
pidstat -t -p <claude>,<nested-agentic-mcp> 5 3
pidstat -d -p <claude>,<nested-agentic-mcp> 5 3
lsof -Pan -p <claude> -p <nested-agentic-mcp>
ss -xainp | rg '<claude>|<nested-agentic-mcp>'
timeout 20 strace -ff -tt -p <claude> -p <nested-agentic-mcp> -e trace=read,write,poll,ppoll,epoll_wait,recvfrom,sendto,wait4,close
```

Interpretation:

| Observation | Meaning |
| --- | --- |
| Tool still `running`, Claude alive, nested MCP alive | Unbounded Claude/MCP stall or uncancelled tool future. |
| Parent future logs drop/cancel but child lives | Detached worker task bug. |
| Claude dies but nested MCP lives | Missing process-group cleanup. |
| `pipe_write` or queued pipe bytes | stdout/stderr deadlock. |
| Claude PGID equals opencode PGID | Group kill is unsafe until process-group isolation lands. |
| Claude PGID differs from opencode PGID | Safe basis for process-group cleanup. |

## Minimal Fix Sequence

Only implement after failing repro tests exist.

1. Make `Session::wait` cancellation-safe: do not remove a `JoinHandle` from `self.tasks` until it has completed.
2. Make `Session::kill` effective after worker startup by keeping process identity/control reachable after the worker takes pipe ownership.
3. Spawn Claude in its own Unix process group with Tokio `Command::process_group(0)`.
4. Make cleanup group-aware: SIGTERM `-pgid`, short grace period, SIGKILL `-pgid`, then wait/reap the direct child.
5. Drain stdout and stderr concurrently in `TextParser` and `SingleJsonParser`.
6. Route `agentic-mcp` tracing explicitly to stderr.
7. Add a configurable `ask_agent` runtime timeout after subprocess cleanup is reliable.
8. Longer term: add cooperative cancellation to `ToolContext` and MCP dispatch if `rmcp` exposes cancellation signals.

## Verification

Focused commands:

```bash
cargo test -p claudecode --test process_lifecycle -- --nocapture
cargo test -p claudecode
cargo test -p coding_agent_tools --test ask_agent_fake_claude -- --nocapture
cargo test -p coding_agent_tools
```

Repo recipes:

```bash
just crate-test claudecode
just crate-test coding_agent_tools
just crate-check claudecode
just crate-check coding_agent_tools
```

Expected final process tree after success, timeout, cancellation, or drop:

```text
opencode
  -> agentic-mcp

No old claude processes.
No old nested agentic-mcp processes.
No persistent zombies.
The tool future resolves or is cancelled with cleanup.
The opencode DB part is no longer indefinitely running.
```
