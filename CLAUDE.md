# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Repository Structure
- `thoughts_tool/` - Flexible thought management using filesystem mounts (Rust CLI)
- `universal_tool/` - Code generation library for multi-interface deployment (Rust library)
- `claudecode_rs/` - Rust SDK for programmatically interacting with Claude Code (Rust library)
- `context/` - Thoughts-based documentation and planning
  - `context/general/` - Root level documents impacting multiple tools/directories in the monorepo
  - `context/thoughts_tool/` - Thoughts-based documents for thoughts_tool
  - `context/universal_tool/` - Thoughts-based documents for universal_tool
  - `context/claudecode_rs/` - Thoughts-based documents for claudecode_rs

## Tool-Specific Guidance
- For thoughts_tool development: See `thoughts_tool/CLAUDE.md`
- For universal_tool development: See `universal_tool/CLAUDE.md`
- For claudecode_rs development: See `claudecode_rs/CLAUDE.md`

## Common Commands

### thoughts_tool
```bash
cd thoughts_tool && make all       # Check, test, and build (silent)
cd thoughts_tool && make check     # Run clippy
cd thoughts_tool && make test      # Run tests
cd thoughts_tool && make build     # Build the project
```

### universal_tool
```bash
cd universal_tool && make all       # Check, test, and build (silent)
cd universal_tool && make check     # Run clippy
cd universal_tool && make test      # Run tests
cd universal_tool && make build     # Build the project

# Or using cargo directly
cd universal_tool && cargo build --workspace --all-features
```

### claudecode_rs
```bash
cd claudecode_rs && make all       # Check, test, and build (silent)
cd claudecode_rs && make check     # Run clippy
cd claudecode_rs && make test      # Run tests
cd claudecode_rs && make build     # Build the project
```

All tools support verbose and normal output variants for their make commands (e.g., `make test-normal`, `make test-verbose`).

## Code Style Guidelines

Rules on comment annotations:

- Keep TODO annotations tagged with severity/priority:
    - TODO(0): Egregious bugs, marked temporarily during development, should never be merged to head
    - TODO(1): Significant architectural flaws, minor bugs
    - TODO(2): Minor design flaws, lacking elegance, missing functionality
    - TODO(3): Minor issues, e.g., lacking unit test coverage

