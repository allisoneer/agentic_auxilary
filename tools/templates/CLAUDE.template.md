# CLAUDE.md - {{crate.name}}

<!-- BEGIN:xtask:autogen header -->
- Crate: {{crate.name}}
- Path: {{crate.path}}
- Role: {{metadata.repo.role}}
- Family: {{metadata.repo.family}}
- Integrations: mcp={{metadata.repo.integrations.mcp}}, logging={{metadata.repo.integrations.logging}}, napi={{metadata.repo.integrations.napi}}
<!-- END:xtask:autogen -->

## Overview

Briefly describe the purpose of this crate and how to use it.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt --package {{crate.name}} --all -- --check
cargo clippy -p {{crate.name}} --all-targets -- -D warnings

# Tests
cargo test -p {{crate.name}}

# Build
cargo build -p {{crate.name}}
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
