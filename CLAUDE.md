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

## Common Commands

### thoughts_tool
```bash
cd thoughts_tool && make all       # Check, test, and build (silent)
cd thoughts_tool && make check     # Run formatting and clippy checks
cd thoughts_tool && make test      # Run tests
cd thoughts_tool && make build     # Build the project
```

### universal_tool
```bash
cd universal_tool && make all       # Check, test, and build (silent)
cd universal_tool && make check     # Run formatting and clippy checks
cd universal_tool && make test      # Run tests
cd universal_tool && make build     # Build the project

# Or using cargo directly
cd universal_tool && cargo build --workspace --all-features
```

### claudecode_rs
```bash
cd claudecode_rs && make all       # Check, test, and build (silent)
cd claudecode_rs && make check     # Run formatting and clippy checks
cd claudecode_rs && make test      # Run tests
cd claudecode_rs && make build     # Build the project
```

### pr_comments
```bash
cd pr_comments && make all       # Check, test, and build (silent)
cd pr_comments && make check     # Run formatting and clippy checks
cd pr_comments && make test      # Run tests
cd pr_comments && make build     # Build the project
```

### gpt5_reasoner
```bash
cd gpt5_reasoner && make all       # Check, test, and build (silent)
cd gpt5_reasoner && make check     # Run formatting and clippy checks
cd gpt5_reasoner && make test      # Run tests
cd gpt5_reasoner && make build     # Build the project

# Useful during debugging:
RUST_LOG=gpt5_reasoner=debug make test
```

### anthropic_async
```bash
cd anthropic_async && make all       # Check, test, and build (silent)
cd anthropic_async && make check     # Run formatting and clippy checks
cd anthropic_async && make test      # Run tests
cd anthropic_async && make build     # Build the project
```

### coding_agent_tools
```bash
cd coding_agent_tools && make all       # Check, test, and build (silent)
cd coding_agent_tools && make check     # Run formatting and clippy checks
cd coding_agent_tools && make test      # Run tests
cd coding_agent_tools && make build     # Build the project
```

All tools support verbose and normal output variants for their make commands (e.g., `make test-normal`, `make test-verbose`).

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

