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
