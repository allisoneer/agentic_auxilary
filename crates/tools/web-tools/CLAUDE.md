# CLAUDE.md - web-tools

<!-- BEGIN:xtask:autogen header -->
- Crate: web-tools
- Path: crates/tools/web-tools/
- Role: tool-lib
- Family: tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Implements web tooling:

- `web_fetch`: downloads a URL, converts HTML to Markdown (or returns text/JSON), optional Haiku summarization.
- `web_search`: semantic search via Exa and returns compact, citable result cards.

Environment:
- `EXA_API_KEY` required for `web_search`
- `ANTHROPIC_API_KEY` required only when `summarize=true` for `web_fetch`

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p web-tools -- --check
cargo clippy -p web-tools --all-targets -- -D warnings

# Tests
cargo test -p web-tools

# Build
cargo build -p web-tools
```
<!-- END:xtask:autogen -->

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
