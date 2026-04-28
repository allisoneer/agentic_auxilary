# opencode-orchestrator-mcp

App-local MCP orchestrator for running OpenCode sessions and handling permission flows.

## Quick Commands

```bash
cd apps/opencode-orchestrator-mcp

just test
just check
just build
```

## Integration tests (environment-gated)

Integration tests are `#[ignore]` by default and require a working `opencode` binary plus local provider configuration.

Use the pinned v1.14.19 bunx lane for live integration validation:

```bash
just smoke-bunx-stable-version
just integration-test-bunx-stable
```

## Local timeout smoke (recommended)

1. `just test`
2. Primary: `just integration-test-one prompt_completes_and_extracts_response`
3. Secondary: `just integration-test-one unknown_command_errors_fast`
4. Optional: `just integration-test-verbose`

## Non-routine tests

`tests/activity_timeout_wiremock.rs` paused-time tests are ignored/flaky and not routine local validation targets.
