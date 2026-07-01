# CLAUDE.md - agentic-outer-dag-bin

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-outer-dag-bin
- Path: apps/agentic-outer-dag/
- Role: app
- Family: tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

`agentic-outer-dag` drives the outer workflow around a feature worktree: it resolves or creates the target worktree, persists run state under `thoughts/<branch>/artifacts/`, runs the ticket-to-PR and PR-comment-resolution phases through the embedded OpenCode supervisor, waits for CodeRabbit review completion, and stops when human input or review is required.

Use `agentic-outer-dag start --ticket <LINEAR-KEY>` to begin a run, `resume` to continue a paused run in the current worktree, and `status` to inspect the persisted state. Use `respond-permission`, `respond-question`, `handoff`, and `reset` to drive the workflow when the supervised agent pauses for operator input.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check agentic-outer-dag-bin

# Tests
just crate-test agentic-outer-dag-bin

# Build
just crate-build agentic-outer-dag-bin
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.

- OpenCode session monitoring/completion detection in `src/opencode/supervisor.rs` is intentionally kept in behavioral parity with
  `apps/opencode-orchestrator-mcp/src/tools.rs` (OrchestratorRunTool). If changing idle gating or transcript settling, update the
  outer-DAG regression tests for ENG-929/ENG-972 and cross-check orchestrator behavior.
- OpenCode v1.17.x `POST /session/{id}/command` is completion-coupled rather than enqueue-and-return. Treat that HTTP call as
  dispatch initiation only: once session start is confirmed via SSE, `/session/status`, or bounded transcript evidence, outer-DAG
  must keep supervising via SSE + `/session/status` and treat later `/command` transport failures as non-terminal warnings.

## Phase 1 live-test ladder (conservative)

Goals: exercise worktree selection/creation, state persistence, and existing-PR observation without:
- creating a PR,
- running `linear_ticket_2_pr`,
- running `resolve_pr_comments`,
- posting Linear handoff comments.

Safe stop-after values for Phase 1:
- `dispatching_ticket_to_pr` — validate the existing-PR guard before any `linear_ticket_2_pr` dispatch.
- `waiting_for_coderabbit` — stop before `dispatching_resolve_pr_comments`.

Do not proceed past `dispatching_resolve_pr_comments` in Phase 1.

Steps:
1. Preview only:

   ```bash
   agentic-outer-dag --dry-run start --ticket ENG-992 --branch <branch>
   ```

2. Force OpenCode startup failure only when a dispatch actually needs OpenCode:

   ```bash
   OPENCODE_BINARY=/does-not-exist agentic-outer-dag start --ticket ENG-992 --branch <branch> --force
   ```

3. Existing-PR guard validation without eager OpenCode startup:

   ```bash
   agentic-outer-dag start --ticket ENG-992 --branch <branch> --stop-after dispatching_ticket_to_pr --no-opencode-dispatch --force
   ```

4. Safety-test dirty/conflict freshness stops without posting Linear comments:

   ```bash
   agentic-outer-dag start --ticket ENG-992 --branch <branch> --no-linear-handoff --force
   ```

5. Stop before `resolve_pr_comments`:

   ```bash
   agentic-outer-dag resume --stop-after waiting_for_coderabbit --no-linear-handoff --no-opencode-dispatch
   ```

Compact `status` output includes `opencode_dispatch_enabled`, `linear_handoff_enabled`, and `linear_handoff_posted`, plus `pr_lookup` diagnostics, so live tests can confirm whether safety suppression was active and recover branch/repo lookup context when PR detection returns no match.
