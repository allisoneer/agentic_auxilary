# CLAUDE.md

Guidance for Claude Code when working with this repository.

## Repository Structure

<!-- BEGIN:xtask:autogen crate-index -->
### agentic-tools

- `agentic-tools-core` (lib) - `crates/agentic-tools/core/`
- `agentic-mcp` (app) - `apps/agentic-mcp/`
- `agentic-tools-mcp` (lib) - `crates/agentic-tools/mcp/`
- `agentic-tools-registry` (lib) - `crates/agentic-tools/registry/`
- `agentic-tools-utils` (lib) - `crates/agentic-tools/utils/`
- `agentic-tools-napi` (binding) - `bindings/node/agentic-tools-napi/`
- `agentic-tools-macros` (lib) - `crates/agentic-tools/macros/`

### infra

- `thoughts-tool` (lib) - `crates/infra/thoughts-core/`
- `agentic_logging` (lib) - `crates/infra/agentic-logging/`

### legacy

- `universal-tool-core` (legacy) - `crates/legacy/universal-tool-core/`
- `universal-tool-macros` (legacy) - `crates/legacy/universal-tool-macros/`
- `universal-tool-integration-tests` (legacy) - `crates/legacy/universal-tool-integration-tests/`

### linear

- `linear-tools` (tool-lib) - `crates/linear/tools/`
- `linear-queries` (lib) - `crates/linear/queries/`
- `linear-schema` (lib) - `crates/linear/schema/`

### meta

- `xtask` (xtask) - `crates/meta/xtask/`

### services

- `claudecode` (lib) - `crates/services/claudecode-rs/`
- `anthropic-async` (lib) - `crates/services/anthropic-async/`
- `exa-async` (lib) - `crates/services/exa-async/`
- `opencode_rs` (lib) - `crates/services/opencode-rs/`

### tools

- `thoughts-bin` (app) - `apps/thoughts/`
- `coding_agent_tools` (tool-lib) - `crates/tools/coding-agent-tools/`
- `gpt5_reasoner` (tool-lib) - `crates/tools/gpt5-reasoner/`
- `pr_comments` (tool-lib) - `crates/tools/pr-comments/`
- `thoughts-mcp-tools` (tool-lib) - `crates/tools/thoughts-mcp-tools/`
- `web-retrieval` (tool-lib) - `crates/tools/web-retrieval/`
<!-- END:xtask:autogen -->

## Working Notes

See [TODO.md](TODO.md) for the human maintainer's ad-hoc notes and thoughts. This file contains ideas, observations, and potential work items that are still loosely scoped—things worth thinking about but not yet defined enough for a formal ticket.

## Common Commands

### All Tools (per-tool directory)
```bash
just crate-check <crate>    # Run formatting and clippy checks for a crate
just crate-test <crate>     # Run tests for a crate
just crate-build <crate>    # Build a crate
```

### Root-level orchestration
```bash
just check          # Check entire workspace (fmt + clippy)
just test           # Test entire workspace
just build          # Build entire workspace
just fmt            # Format entire workspace
just fmt-check      # Check formatting across entire workspace
```

### xtask commands
```bash
just xtask-sync         # Sync autogen content (CLAUDE.md, release-plz.toml)
just xtask-verify       # Verify metadata, policy, and file freshness
just xtask-sync-check   # Check if sync is needed (for CI)
just xtask-verify-check # Full verification including generated files
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

The `tools/agent-wrap.sh` wrapper controls output:
- **minimal** (default locally): Commands print a single `✓ task` line on success; failures show `✗ task failed` plus the tail of output
- **normal** (default in CI): Commands run directly with full output
- **verbose**: Commands run directly with additional nextest verbosity flags

### Git Navigation (Read-Only)

For agents without shell access, these just recipes provide safe, read-only git inspection. All commands use `--no-pager` to avoid interactive hangs. Paths with spaces must be quoted.

| Recipe | Parameters | Description |
|--------|------------|-------------|
| `git-context` | `n="5"` | Snapshot: repo root, branch/HEAD, remotes, status, last N commits |
| `git-log` | `n="20"` `path=""` | Commit history, optionally scoped to a path |
| `git-diff` | `area="both"` `format="stat"` `path=""` | Diffs with flexible scope and output format |
| `git-blame` | `file` `start=""` `end=""` | Line authorship, optionally limited to line range |
| `git-show` | `ref` `path=""` | Commit details (ref only) or file contents (ref + path) |
| `git-files` | `patterns=""` | List tracked files, optionally filtered by pathspecs |

**git-diff parameters:**
- `area`: `both` | `working` | `staged` | `head`
- `format`: `stat` | `patch` | `name-only` | `name-status`

**Examples:**

```bash
just git-context
just git-log 30 rust/
just git-diff working patch
just git-diff staged name-status "frontend/src/"
just git-blame README.md
just git-blame "src/main.rs" 10 50
just git-show HEAD
just git-show HEAD rust/Cargo.toml
just git-files
just git-files "*.md docs/"
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

