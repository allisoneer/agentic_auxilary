# CLAUDE.md - attention-kernel

<!-- BEGIN:xtask:autogen header -->
- Crate: attention-kernel
- Path: crates/attention/kernel/
- Role: lib
- Family: attention
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Defines transport- and persistence-independent Attention domain types and invariants for identities, source records, work items, signals, reminders, lifecycle transitions, and default Inbox projections. Use the exported types to construct or reconstruct validated domain state, apply lifecycle methods, and derive Inbox membership with `is_in_default_inbox` or `inbox_entry`; keep I/O, scheduling, and protocol concerns in higher layers.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
just crate-check attention-kernel

# Tests
just crate-test attention-kernel

# Build
just crate-build attention-kernel
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
