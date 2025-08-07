# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Repository Structure
- `thoughts_tool/` - Flexible thought management using filesystem mounts (Rust CLI)
- `universal_tool/` - Code generation library for multi-interface deployment (Rust library)
- `context/` - Thoughts-based documentation and planning
  - `context/general/` - Root level documents impacting multiple tools/directories in the monorepo
  - `context/thoughts_tool/` - Thoughts-based documents for thoughts_tool
  - `context/universal_tool/` - Thoughts-based documents for universal_tool

## Tool-Specific Guidance
- For thoughts_tool development: See `thoughts_tool/CLAUDE.md`
- For universal_tool development: See `universal_tool/CLAUDE.md`

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

Both tools support verbose and normal output variants for their make commands (e.g., `make test-normal`, `make test-verbose`).