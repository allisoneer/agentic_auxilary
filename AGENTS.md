# AGENTS.md

Repository-specific facts for OpenAI-oriented tooling live here. Behavioral rules, task discipline, and generic verification expectations should stay in the active system prompt rather than being duplicated in this file. Repo structure, commands, generated-file facts, and repository-specific conventions belong here.

## Repository Structure

### agentic-tools

- `agentic-tools-core` (lib) - `crates/agentic-tools/core/`
- `agentic-mcp` (app) - `apps/agentic-mcp/`
- `agentic-tools-mcp` (lib) - `crates/agentic-tools/mcp/`
- `agentic-tools-registry` (lib) - `crates/agentic-tools/registry/`
- `agentic-tools-utils` (lib) - `crates/agentic-tools/utils/`
- `opencode-orchestrator-mcp` (app) - `apps/opencode-orchestrator-mcp/`
- `agentic-tools-napi` (binding) - `bindings/node/agentic-tools-napi/`
- `agentic-tools-macros` (lib) - `crates/agentic-tools/macros/`

### infra

- `agentic-config` (lib) - `crates/infra/agentic-config/`
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

- `agentic-bin` (app) - `apps/agentic/`
- `thoughts-bin` (app) - `apps/thoughts/`
- `message-optimizer-bin` (app) - `apps/message-optimizer/`
- `coding_agent_tools` (tool-lib) - `crates/tools/coding-agent-tools/`
- `gpt5_reasoner` (tool-lib) - `crates/tools/gpt5-reasoner/`
- `pr_comments` (tool-lib) - `crates/tools/pr-comments/`
- `review_tools` (tool-lib) - `crates/tools/review-tools/`
- `message_optimizer` (tool-lib) - `crates/tools/message-optimizer/`
- `thoughts-mcp-tools` (tool-lib) - `crates/tools/thoughts-mcp-tools/`
- `web-retrieval` (tool-lib) - `crates/tools/web-retrieval/`

## Common Commands

### Per-crate commands

```bash
just crate-check <crate>    # Run formatting and clippy checks for a crate
just crate-test <crate>     # Run tests for a crate
just crate-build <crate>    # Build a crate
just crate-run <crate>      # Run a crate
```

### Workspace commands

```bash
just check          # Check entire workspace (fmt-check + clippy)
just fix            # Auto-fix clippy warnings across workspace
just test           # Run tests for entire workspace
just test-integration  # Run tests including #[ignore] integration tests (sets THOUGHTS_INTEGRATION_TESTS=1)
just build          # Build entire workspace
just fmt            # Format entire workspace
just fmt-check      # Check formatting across entire workspace
```

### xtask commands

```bash
just xtask-sync         # Sync generated repo metadata files (CLAUDE.md, release-plz.toml, README.md, justfile)
just xtask-verify       # Verify metadata, policy, and file freshness
just xtask-sync-check   # Check if sync is needed (for CI)
just xtask-verify-check # Full verification including generated files
```

`xtask-sync` updates generated repo metadata such as `CLAUDE.md`, `release-plz.toml`, `README.md`, and `justfile`. `AGENTS.md` is not currently auto-synced, so refresh it manually when repo facts change.

### Endpoint coverage (opencode-rs SDK)

```bash
just endpoint-coverage          # Print opencode-rs API endpoint coverage report
just endpoint-coverage-check    # Fail if coverage regresses
just endpoint-coverage-json     # JSON output for tooling
```

### Schema generation

```bash
just schema-generate    # Regenerate agentic.schema.json from Rust types
```

### Vendored Codex

`vendor/codex/` is a foreign vendored subtree excluded from the root workspace. Do not edit it as a first-class workspace member.

```bash
just codex-check    # Check vendored Codex workspace
just codex-build    # Build vendored Codex CLI
just codex-test     # Run vendored Codex tests (best-effort)
just codex-run -- <args>  # Run the vendored codex binary
```

## Toolchain and Formatter Quirks

- Stable toolchain pinned to `1.93.0` (`rust-toolchain.toml`).
- **Formatting requires nightly**: `just fmt` and `just fmt-check` run `cargo +nightly fmt`. Running `cargo fmt` without `+nightly` will use the wrong edition settings and fail.
- `rustfmt.toml` uses `edition = "2024"` and `imports_granularity = "Item"` — don't change these.
- `just test` runs `mcp-test` (MCP schema validation via `npx @modelcontextprotocol/inspector`) before nextest. Node.js / `npx` must be available.

## Lint Policy

- `.unwrap()` and `.expect()` are banned workspace-wide (clippy `warn`). Use `?` or explicit error handling.
- `clippy.toml` allows `.unwrap()` / `.expect()` inside `#[cfg(test)]` test code only.
- Every `unsafe` block requires a `// SAFETY:` comment (`undocumented_unsafe_blocks = "warn"`).
- `#[allow(...)]` is banned; use `#[expect(...)]` instead (`allow_attributes = "warn"`).
- `Arc::clone(&x)` is required over `x.clone()` for ref-counted types (`clone_on_ref_ptr = "warn"`).
- Workspace lint inheritance: add `[lints]\nworkspace = true` when creating or modifying crate `Cargo.toml` files.

## Output Modes

The `tools/agent-wrap.sh` wrapper controls command output:

- `minimal` (default locally): print a single success line; failures show a short tail
- `normal` (default in CI): show full command output
- `verbose`: show direct command output with extra nextest verbosity

Examples:

```bash
just test
OUTPUT_MODE=normal just test
OUTPUT_MODE=verbose just test
RUST_LOG=gpt5_reasoner=debug just test
```

## Git Write Recipes

For agents without shell access, these just recipes provide git-aware move/remove operations:

| Recipe | Parameters | Description |
| --- | --- | --- |
| `git-mv` | `src` `dst` `mkdir_parents="true"` | Move/rename a tracked path with git mv |
| `git-rm` | `path` `force="false"` `recursive="auto"` | Remove a tracked path with git rm |

## Read-Only Git Inspection Recipes

For agents without shell access, these Just recipes provide safe, read-only git inspection. All commands use `--no-pager` to avoid interactive hangs. Paths with spaces must be quoted.

| Recipe | Parameters | Description |
| --- | --- | --- |
| `git-context` | `n="5"` | Repo root, branch or HEAD, remotes, status, recent commits |
| `git-log` | `n="20"` `path=""` | Commit history, optionally scoped to a path |
| `git-diff` | `area="both"` `format="stat"` `path=""` | Diff output with scope and format controls |
| `git-blame` | `file` `start=""` `end=""` | Line authorship, optionally limited to a range |
| `git-show` | `ref` `path=""` | Commit details or file contents at a ref |
| `git-files` | `patterns=""` | Tracked files, optionally filtered by pathspecs |

`git-diff` supports:

- `area`: `both` | `working` | `staged` | `head`
- `format`: `stat` | `patch` | `name-only` | `name-status`

Examples:

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

## README Version Sync

Run locally to update README versions:

```bash
cargo run -p xtask -- readme-sync
```

Dry run (prints updated README content to stdout, does not write the file):

```bash
cargo run -p xtask -- readme-sync --dry-run
```

Strict mode (fails on malformed markers or unknown crates):

```bash
AUTODEPS_STRICT=1 cargo run -p xtask -- readme-sync
```

## Project Context Files

These files contain important project-level context that should be read and kept up to date:

| File | Purpose | When to Read | When to Update |
|------|---------|--------------|----------------|
| `TODO.md` | Living work queue: investigating, ready, blocked/sequenced, to-plan, to-classify | When planning new work, checking dependencies, understanding what's blocked or in-flight | When finishing work that unblocks other items, discovering new work items, or changing priorities |
| `workflow.md` | Visual guide to agent architecture: orchestrator → session agents → sub-agents, tool matrices, decision flowchart | When understanding how agents/commands/tools relate, onboarding to the system, or debugging agent behavior | When adding new commands, changing tool availability, or modifying the agent hierarchy |

**TODO.md categories:**
- `Currently investigating` — active research
- `Researched / Ready for implementation` — can be picked up now
- `Blocked / Sequenced` — has dependencies, do in order
- `To plan/design` — needs design work before implementation
- `To classify/investigate` — needs triage
- `To validate` — needs verification

**workflow.md sections:**
- Level 0: Orchestrator tools and spawning
- Level 1: Session agent variants (Normal, Bash, Linear, Playwright, Review) and their tool counts
- Level 2: Sub-agent matrix (Locator/Analyzer × Codebase/Thoughts/References/Web)
- GPT-5 Reasoner integration
- Decision flowchart for choosing the right agent/command

## Code Style Guidance

Repository-specific TODO annotations use these severity tags:

- `TODO(0)`: egregious bugs; temporary during development and should not merge to head
- `TODO(1)`: significant architectural flaws or minor bugs
- `TODO(2)`: minor design flaws, missing functionality, or elegance issues
- `TODO(3)`: minor issues such as missing unit test coverage
