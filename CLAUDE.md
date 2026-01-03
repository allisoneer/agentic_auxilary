# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Repository Structure
- `thoughts_tool/` - Flexible thought management using filesystem mounts (Rust CLI)
- `universal_tool/` - Code generation library for multi-interface deployment (Rust library)
- `claudecode_rs/` - Rust SDK for programmatically interacting with Claude Code (Rust library)
- `pr_comments/` - Fetch GitHub PR comments with resolution filtering (CLI + MCP, Rust)
- `gpt5_reasoner/` - Two-phase GPT-5 prompt optimizer/executor with directory-aware file ingestion (CLI + MCP, Rust)
- `anthropic_async/` - Asynchronous Anthropic API client for Rust (library)
- `coding_agent_tools/` - Coding agent tools for CLI + MCP (first tool: ls) (Rust CLI + MCP)
- `context/` - Thoughts-based documentation and planning
  - `context/general/` - Root level documents impacting multiple tools/directories in the monorepo
  - `context/thoughts_tool/` - Thoughts-based documents for thoughts_tool
  - `context/universal_tool/` - Thoughts-based documents for universal_tool
  - `context/claudecode_rs/` - Thoughts-based documents for claudecode_rs
  - `context/gpt5_reasoner/` - Thoughts-based documents for gpt5_reasoner

## Tool-Specific Guidance
- For thoughts_tool development: See `thoughts_tool/CLAUDE.md`
- For universal_tool development: See `universal_tool/CLAUDE.md`
- For claudecode_rs development: See `claudecode_rs/CLAUDE.md`
- For pr_comments development: See `pr_comments/CLAUDE.md`
- For gpt5_reasoner development: See `gpt5_reasoner/CLAUDE.md`
- For coding_agent_tools development: See `coding_agent_tools/CLAUDE.md`
- TODO(2): Add `anthropic_async/CLAUDE.md` (file currently missing)

## Common Commands

### All Tools (per-tool directory)
```bash
cd <tool> && just check     # Run formatting and clippy checks
cd <tool> && just test      # Run tests
cd <tool> && just build     # Build the project
cd <tool> && just fmt       # Format code
```

### Root-level orchestration
```bash
just check          # Check all tools
just test           # Test all tools
just build          # Build all tools
just fmt-all        # Format all code
just fmt-check-all  # Check formatting across all tools
```

### Output modes
```bash
# Default: minimal (quiet on success, verbose on failure)
just test

# Normal mode: full cargo output
OUTPUT_MODE=normal just test

# Verbose mode: extra verbosity flags
OUTPUT_MODE=verbose just test

# Debugging example
RUST_LOG=gpt5_reasoner=debug just test
```

## README version sync (xtask)

- Run locally to update README versions:
```bash
cargo run -p xtask -- readme-sync
```
- Dry run (prints updated README content to stdout, does not write file):
```bash
cargo run -p xtask -- readme-sync --dry-run
```
- Strict mode (fail on malformed markers or unknown crates):
```bash
AUTODEPS_STRICT=1 cargo run -p xtask -- readme-sync
```

## Code Style Guidelines

Rules on comment annotations:

- Keep TODO annotations tagged with severity/priority:
    - TODO(0): Egregious bugs, marked temporarily during development, should never be merged to head
    - TODO(1): Significant architectural flaws, minor bugs
    - TODO(2): Minor design flaws, lacking elegance, missing functionality
    - TODO(3): Minor issues, e.g., lacking unit test coverage

