# ENG-758: Fix claudecode stdout/stderr pipe deadlock risk in ask_agent

Linear: https://linear.app/general-wisdom/issue/ENG-758/fix-claudecode-stdoutstderr-pipe-deadlock-risk-in-ask-agent

Related research: `agentic-mcp-lifecycle-investigation.md`

## Goal

Fix the concrete `ask_agent` hang risk where `claudecode` reads stdout to EOF before reading stderr in text and single-JSON output modes.

This is the first-priority PR because it is the only identified code bug that can directly cause ordinary non-cancelled `ask_agent` calls to hang.

## Background

`ask_agent` uses `claudecode` with `OutputFormat::Text`.

The current parsers read streams sequentially:

- `crates/services/claudecode-rs/src/stream.rs:78` - `SingleJsonParser::parse`
- `crates/services/claudecode-rs/src/stream.rs:118` - `TextParser::parse`

If Claude or a wrapper writes enough stderr to fill the pipe while stdout remains open, the child can block on stderr and the parent can wait forever for stdout EOF.

## Scope

- Read stdout and stderr concurrently in `TextParser::parse`.
- Read stdout and stderr concurrently in `SingleJsonParser::parse`.
- Preserve existing result semantics for stderr content.
- Add focused tests that would fail or time out if stderr is not drained concurrently.

## Out Of Scope

- Do not add `ask_agent` runtime timeouts in this PR.
- Do not change process groups or lifecycle cleanup in this PR.
- Do not change MCP cancellation plumbing in this PR.

## Starting Files

- `crates/services/claudecode-rs/src/stream.rs`
- Optional focused tests in the same module under `#[cfg(test)]`

## Test Ideas

- Use `tokio::io::duplex` or another async test reader to model stdout staying open while stderr has data available.
- Wrap parser execution in `tokio::time::timeout` so a regression fails quickly instead of hanging the test suite.
- Cover both `TextParser` and `SingleJsonParser`.

## Acceptance Criteria

- `TextParser` cannot deadlock when stderr receives more than a pipe buffer while stdout remains open.
- `SingleJsonParser` cannot deadlock for the same pattern.
- Existing parser behavior is preserved for stdout-only and stderr-present cases.
- `cargo test -p claudecode` passes.
