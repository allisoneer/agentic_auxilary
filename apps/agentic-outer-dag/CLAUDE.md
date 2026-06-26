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
cargo fmt -p agentic-outer-dag-bin -- --check
cargo clippy -p agentic-outer-dag-bin --all-targets -- -D warnings

# Tests
cargo test -p agentic-outer-dag-bin

# Build
cargo build -p agentic-outer-dag-bin
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.

## Phase 1 live-test ladder (conservative)

Goals: exercise worktree selection/creation, state persistence, and existing-PR observation without:
- creating a PR,
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
   agentic-outer-dag start --ticket ENG-992 --branch <branch> --stop-after dispatching_ticket_to_pr --force
   ```
4. Safety-test dirty/conflict freshness stops without posting Linear comments:
   ```bash
   agentic-outer-dag start --ticket ENG-992 --branch <branch> --no-linear-handoff --force
   ```
5. Stop before `resolve_pr_comments`:
   ```bash
   agentic-outer-dag resume --stop-after waiting_for_coderabbit --no-linear-handoff
   ```

Compact `status` output includes both `linear_handoff_enabled` and `linear_handoff_posted` so live tests can confirm whether safety suppression was active and whether a handoff comment was actually posted.
