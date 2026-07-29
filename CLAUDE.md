# CLAUDE.md

Canonical repository guidance for AI-assisted work in this repository. Keep repo-specific instructions here. Preserve the xtask-managed crate-index block unchanged.

## Repository Structure

<!-- BEGIN:xtask:autogen crate-index -->
### agentic-tools

- `agentic-tools-utils` (lib) - `crates/agentic-tools/utils/`
- `agentic-tools-core` (lib) - `crates/agentic-tools/core/`
- `agentic-tools-mcp` (lib) - `crates/agentic-tools/mcp/`
- `agentic-mcp` (app) - `apps/agentic-mcp/`
- `agentic-tools-registry` (lib) - `crates/agentic-tools/registry/`
- `opencode-orchestrator-mcp` (app) - `apps/opencode-orchestrator-mcp/`
- `agentic-tools-napi` (binding) - `bindings/node/agentic-tools-napi/`
- `agentic-tools-macros` (lib) - `crates/agentic-tools/macros/`

### attention

- `attention-kernel` (lib) - `crates/attention/kernel/`
- `attention-protocol` (lib) - `crates/attention/protocol/`
- `attention-turso` (lib) - `crates/attention/turso/`

### infra

- `agentic-config` (lib) - `crates/infra/agentic-config/`
- `gwt-worktree` (lib) - `crates/infra/gwt-worktree/`
- `agentic_logging` (lib) - `crates/infra/agentic-logging/`
- `thoughts-tool` (lib) - `crates/infra/thoughts-core/`

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

- `opencode_rs` (lib) - `crates/services/opencode-rs/`
- `claudecode` (lib) - `crates/services/claudecode-rs/`
- `anthropic-async` (lib) - `crates/services/anthropic-async/`
- `exa-async` (lib) - `crates/services/exa-async/`

### tools

- `agentic-bin` (app) - `apps/agentic/`
- `agentic-outer-dag-bin` (app) - `apps/agentic-outer-dag/`
- `pr_comments` (tool-lib) - `crates/tools/pr-comments/`
- `thoughts-bin` (app) - `apps/thoughts/`
- `coding_agent_tools` (tool-lib) - `crates/tools/coding-agent-tools/`
- `discord-tools` (tool-lib) - `crates/tools/discord-tools/`
- `gpt5_reasoner` (tool-lib) - `crates/tools/gpt5-reasoner/`
- `review_tools` (tool-lib) - `crates/tools/review-tools/`
- `thoughts-mcp-tools` (tool-lib) - `crates/tools/thoughts-mcp-tools/`
- `agentic-workspace-tools` (tool-lib) - `crates/tools/workspace-tools/`
- `web-retrieval` (tool-lib) - `crates/tools/web-retrieval/`
- `message-optimizer-bin` (app) - `apps/message-optimizer/`
- `message_optimizer` (tool-lib) - `crates/tools/message-optimizer/`
<!-- END:xtask:autogen -->

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
just check             # Check entire workspace (fmt-check + clippy)
just fix               # Auto-fix clippy warnings across workspace
just test              # Local: mcp-test + nextest (requires Node/npx)
just test-ci           # CI-safe: nextest only (no Node/npx)
just test-integration  # Local: run tests including #[ignore] integration tests
just test-thoughts-ignored-ci # CI-safe: run ignored-only thoughts tests
just build             # Build entire workspace
just fmt               # Format entire workspace
just fmt-check         # Check formatting across entire workspace
```

### xtask commands

```bash
just xtask-sync         # Sync generated repo metadata files (CLAUDE.md, release-plz.toml, mise.toml, README.md, justfile, agentic.schema.json)
just xtask-verify       # Verify metadata, policy, and file freshness
just xtask-sync-check   # Check if sync is needed (for CI)
just xtask-verify-check # Full verification including generated files
```

`xtask-sync` updates generated repo metadata such as root/per-crate `CLAUDE.md`, `release-plz.toml`, `mise.toml`, `README.md`, `justfile`, and `agentic.schema.json`. Managed internal tool versions in `mise.toml` are sourced from `tools/published-versions.toml`.

Release PRs labeled `release` trigger `.github/workflows/readme-sync.yml`, which runs full `cargo run -p xtask -- sync` on same-repo PR heads and auto-commits only xtask-managed generated outputs when they are stale.

`cargo run -p xtask -- release-plz-preflight` is the release-plz-only first-publish guard. It checks crates configured to publish by `tools/policy.toml` against crates.io before the release-plz action runs. If it fails, either first-publish the missing crate locally with `cargo publish -p <crate>` or mark it unpublished in `tools/policy.toml` and rerun `cargo run -p xtask -- sync`.

`tools/published-versions.toml` is the source of truth for the last fully published managed internal binary versions. `xtask verify --check` enforces coherence between that contract, generated `mise.toml`, and `mise.lock`.

`mise.lock` should stay coherent with `tools/published-versions.toml`. On `main`, post-publication automation is intended to update the contract, regenerate `mise.toml`, and refresh `mise.lock` together after stable GitHub Release assets exist. For local/manual contract edits, regenerate it with `MISE_LOCKED=0 mise lock`.

Workflow: `.github/workflows/published-versions-sync.yml`
- Trigger: stable GitHub Release published, plus manual `workflow_dispatch`
- Behavior: verifies required assets, updates `tools/published-versions.toml`, regenerates `mise.toml` and `mise.lock`, and pushes directly to `main`; if repository rules block the direct push, it opens a fallback PR instead
- If it fails due to permissions, branch protection, or asset/lock issues, fix the reported cause, rerun the workflow or apply the same sequence locally (`cargo run -p xtask -- published-versions --tag <tag>`, `cargo run -p xtask -- sync`, `MISE_LOCKED=0 mise lock`), then push the resulting three-file update or open a PR

`.github/workflows/release.yml` is generated by cargo-dist but intentionally carries a manual host-job patch: GitHub Releases must be created with the `RELEASE_PLZ` GitHub App token, not the default `GITHUB_TOKEN`, because GitHub suppresses downstream workflow runs for events created by `GITHUB_TOKEN`. If cargo-dist regenerates `release.yml`, preserve the in-file `MANUAL PATCH` comments, the `Generate GitHub token` step, and the `GH_TOKEN` / `GITHUB_TOKEN` overrides on the `Create GitHub Release` step; otherwise `published-versions-sync.yml` will not run automatically on future releases.

Release policy is conservative by default: libraries publishable to crates.io do not get GitHub-release-facing tags unless explicitly allowlisted in `tools/policy.toml`. The intended tagged packages are the distributed apps, while `message-optimizer-bin` remains an intentional outlier: it keeps release-plz tagging enabled but is not being normalized into the broader shipped-app or `mise` workflows in this task.

### Endpoint coverage (opencode-rs SDK)

```bash
just endpoint-coverage       # Print opencode-rs API endpoint coverage report
just endpoint-coverage-check # Fail if coverage regresses
just endpoint-coverage-json  # JSON output for tooling
```

### Schema generation

```bash
just schema-generate    # Regenerate agentic.schema.json from Rust types
```

### Vendored Codex

`vendor/codex/` is a foreign vendored subtree excluded from the root workspace. Do not edit it as a first-class workspace member.

```bash
just codex-check          # Check vendored Codex workspace
just codex-build          # Build vendored Codex CLI
just codex-test           # Run vendored Codex tests (best-effort)
just codex-run -- <args>  # Run the vendored codex binary
```

## Toolchain and Formatter Quirks

- Stable toolchain pinned to `1.96.1` (`rust-toolchain.toml`).
- Formatting requires nightly: `just fmt` and `just fmt-check` run `cargo +nightly fmt`. Running `cargo fmt` without `+nightly` uses the wrong edition settings and fails.
- `rustfmt.toml` uses `edition = "2024"` and `imports_granularity = "Item"`; do not change these.
- `just test` runs `mcp-test` (MCP schema validation via `npx @modelcontextprotocol/inspector`) before nextest. Node.js and `npx` must be available.

## Lint Policy

- `.unwrap()` and `.expect()` are banned workspace-wide (clippy `warn`). Use `?` or explicit error handling.
- `clippy.toml` allows `.unwrap()` and `.expect()` inside `#[cfg(test)]` test code only.
- Every `unsafe` block requires a `// SAFETY:` comment (`undocumented_unsafe_blocks = "warn"`).
- `#[allow(...)]` is banned; use `#[expect(...)]` instead (`allow_attributes = "warn"`).
- `Arc::clone(&x)` is required over `x.clone()` for ref-counted types (`clone_on_ref_ptr = "warn"`).
- Workspace lint inheritance: add `[lints]` with `workspace = true` when creating or modifying crate `Cargo.toml` files.

## cli_just_execute Output Paging

Interactive local root `just` recipes may use `tools/agent-wrap.sh` for a single success checkmark or failure smart-tail.

The wrapper auto-disables in CI and when stdout is not a TTY, so tooling such as `cli_just_execute` still receives raw output suitable for paging.

`cli_just_execute` returns stdout/stderr in pages (default 200 lines per page; configurable via `[cli_tools].just_execute_page_lines`).

- If `has_more=true`, call `cli_just_execute` again with the exact same params for the next cached page.
- Use `rerun=true` to force a fresh execution.
- Set `AGENTIC_WRAP=0` to force full passthrough output locally.

Examples:

```bash
just test
just test-ci
just check
cli_just_execute recipe=test
AGENTIC_WRAP=0 just check
RUST_LOG=gpt5_reasoner=debug just test
```

## Git Write Recipes

For agents without shell access, these just recipes provide git-aware move/remove operations:

| Recipe | Parameters | Description |
| --- | --- | --- |
| `git-mv` | `src` `dst` `mkdir_parents="true"` | Move/rename a tracked path with git mv |
| `git-rm` | `path` `force="false"` `recursive="auto"` | Remove a tracked path with git rm |

## Read-Only Git Inspection Recipes

For agents without shell access, these just recipes provide safe, read-only git inspection. All commands use `--no-pager` to avoid interactive hangs. Paths with spaces must be quoted.

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

## Review Workflow

See `workflow.md` -> "Code Review (/review)" for:

- Dedicated Review agent (`Review`)
- Tool isolation rules for `review_*`
- End-to-end `/review` usage

## Code Style Guidance

Repository-specific TODO annotations use these severity tags:

- `TODO(0)`: egregious bugs; temporary during development and should not merge to head
- `TODO(1)`: significant architectural flaws or minor bugs
- `TODO(2)`: minor design flaws, missing functionality, or elegance issues
- `TODO(3)`: minor issues such as missing unit test coverage
