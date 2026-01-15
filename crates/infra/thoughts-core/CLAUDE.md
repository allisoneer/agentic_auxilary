# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust CLI application called "thoughts" - a flexible thought management tool that helps developers organize notes and documentation across git repositories using filesystem mounts (mergerfs on Linux, fuse-t on macOS) with a three-space architecture:
- **thoughts/** - Personal workspace for work documents
- **context/** - Team-shared documentation repositories
- **references/** - Read-only external code repositories

## Common Development Commands

### Building and Testing
```bash
# Default targets (silent if successful)
just check      # Run formatting and clippy checks
just test       # Run all tests
just build      # Build the project
just fmt        # Format code
just fmt-check  # Check formatting

# Output mode variants
OUTPUT_MODE=normal just test    # Normal output
OUTPUT_MODE=verbose just test   # Verbose output

# Specific test types via cargo
cargo test --lib                # Unit tests only
THOUGHTS_INTEGRATION_TESTS=1 just test  # Integration tests included
```

## High-Level Architecture

### Command Structure
The application uses a modular command pattern with each CLI command implemented in `/src/commands/`:
- `init` - Initialize thoughts for a repository (creates three symlinks)
- `sync` - Sync git-backed mounts
- `status` - Show mount status
- `mount/*` - Context mount management (add, remove, list, update, clone)
- `references/*` - Reference repository management (add, remove, list, sync)
- `work/*` - Work organization (init, complete, list)
- `config/*` - Configuration management (create, show, edit, validate)
  - Note: `get` and `set` commands exist in the codebase but are not wired to the CLI

### Platform Abstraction
The application supports both Linux (mergerfs) and macOS (fuse-t) through:
- Platform detection at runtime (`/src/platform/detector.rs`)
- Strategy pattern for mount implementations (`/src/mount/`)
- Conditional compilation with feature flags

### Configuration System
- JSON-based configuration (v2 format) in `.thoughts/config.json`
- Automatic v1 to v2 migration via `DesiredState` abstraction
- Type-safe mount identification using `MountSpace` enum
- Interactive editing with `$EDITOR`
- Repository mapping system (`RepoMappingManager`) for URL to local path mappings
- Three-space architecture configuration:
  - `thoughts_mount` - Personal workspace repository
  - `context_mounts` - Team-shared documentation
  - `references` - External code repository URLs

#### Configuration API Guidelines (v2 Preferred)

**For new code, always prefer v2 APIs:**

Write operations:
- ✅ Use `ensure_v2_default()` instead of `ensure_default()`
- ✅ Use `save_v2_validated()` instead of `save_v2()` or `save()`
- ✅ Always validate with `validate_v2_hard()` before saving

Read operations:
- ✅ Use `load_desired_state()` for version-agnostic reads
- ✅ Use `load_v2_or_bail()` when v2 is required
- ⚠️ Use `load()` only for v1-specific operations (deprecated)

Version detection:
- ✅ Use `peek_config_version()` for lightweight version checks

Migration pattern for commands:
```rust
let was_v1 = matches!(mgr.peek_config_version()?, Some(v) if v == "1.0");
let mut cfg = mgr.ensure_v2_default()?; // auto-migrates if needed
// ... modify cfg ...
let warnings = mgr.save_v2_validated(&cfg)?;
for w in warnings { eprintln!("Warning: {}", w); }
if was_v1 {
    eprintln!("Upgraded to v2 config. See MIGRATION_V1_TO_V2.md");
}
```

**V1 APIs are maintained for backward compatibility but should not be used in new code.**

### Testing Strategy
- Unit tests embedded in modules (`#[cfg(test)]`)
- Integration tests in `/tests/` directory requiring `THOUGHTS_INTEGRATION_TESTS=1`
- Mock implementations for testing mount operations
- Temporary directories for filesystem testing

## Key Implementation Notes

1. **Rust Edition**: Uses Rust 2024 edition - ensure compatibility when adding dependencies
2. **Error Handling**: Uses `anyhow` for application errors and `thiserror` for library errors
3. **Async Runtime**: Uses `tokio` for async operations with full features
4. **Git Integration**: Hybrid backend - see Git Architecture section below
5. **Platform Features**: Platform-specific code uses conditional compilation (`#[cfg(target_os = "linux")]`)
6. **Build Warnings**: Currently suppresses warnings in silent mode (TODO: fix all warnings)

### Git Architecture

The tool uses a hybrid git backend to ensure compatibility with SSH agents like 1Password:

| Operation | Backend | Rationale |
|-----------|---------|-----------|
| Clone | gitoxide (`gix`) | Network op via system SSH |
| Fetch | Shell `git fetch` | Network op via system SSH |
| Push | Shell `git push` | Network op via system SSH |
| Stage/Commit/Rebase | git2 | Local ops, proven, low risk |
| Utils/Discovery | git2 | Local ops, no change needed |

**Why this approach:**
- **1Password compatibility**: libssh2 (used by git2) doesn't trigger 1Password approval dialogs. System git and gitoxide both use the system SSH client which properly triggers 1Password prompts.
- **Minimal risk**: Local operations (staging, rebase) remain on proven git2 code.
- **Worktree support**: Preserved through git2-based detection.

### Git HTTP Backend

HTTPS clone operations use gitoxide (`gix`) with the `blocking-http-transport-reqwest-rust-tls` feature.

**Rationale:**
- Pure Rust HTTP stack: reqwest + rustls (no curl, no OpenSSL)
- Cross-platform stability with no system dependencies
- Consistent with `anthropic_async` HTTP client configuration

SSH behavior remains unchanged and continues to use the system SSH client for 1Password agent compatibility.

### Test Environment Variables

| Variable | Purpose | When Set |
|----------|---------|----------|
| `THOUGHTS_INTEGRATION_TESTS=1` | Enables integration tests (file:// clone tests) | CI PR runs, local dev |
| `THOUGHTS_NETWORK_TESTS=1` | Enables network-dependent tests (HTTPS clones) | Nightly CI only |

**Running all tests locally:**
```bash
THOUGHTS_INTEGRATION_TESTS=1 THOUGHTS_NETWORK_TESTS=1 just test
```

### Known Issue (v0.4.0): Fast-forward pull left staged changes after sync

**Affected version:** v0.4.0

**Symptom:**
- After `thoughts references sync` or `thoughts sync`, `git status` shows staged changes even though no local edits were made.
- Root cause: A libgit2 behavior where `checkout_head()` no-ops if called immediately after `set_head()`, leaving the working tree at the old commit while HEAD points to the new commit.

**Fixed in:** v0.4.1+ using `git reset --hard` semantics via libgit2's `reset(Hard)` for atomic ref/index/worktree updates.

**One-time manual fix for affected repositories:**

If on a normal branch (most common):
```bash
git reset --hard HEAD
```
This updates the index and working tree to match the commit your branch already points to.

If you see "HEAD detached" in `git status`:
1. Re-attach to your branch:
   ```bash
   git checkout <your-branch>
   ```
2. Then update your worktree to the branch tip:
   ```bash
   git reset --hard HEAD
   ```

**Notes:**
- The tool keeps a safety gate (`is_worktree_dirty()`) and will not reset over local changes; commit or stash first.
- Network operations still use the system `git` client for 1Password SSH compatibility; local operations use libgit2.

## Major Dependencies

- **clap** - CLI framework with derive macros for argument parsing
- **serde/serde_json** - JSON serialization and deserialization
- **tracing/tracing-subscriber** - Structured logging framework
- **dirs** - Cross-platform directory paths
- **nix** - Unix system calls for mount operations
- **colored** - Terminal color output support
- **anyhow** - Flexible error handling for applications
- **thiserror** - Custom error types with automatic From implementations
- **tokio** - Async runtime with full features
- **git2** - Git local operations (staging, commit, rebase, repo discovery)
- **gix** - Git clone operations (uses system SSH for 1Password compatibility)

## Advanced Features

### Git Worktree Support

The tool now fully supports git worktrees through automatic detection and smart initialization:

- `is_worktree()` - Detects if current directory is a worktree (src/git/utils.rs:25)
- `get_main_repo_for_worktree()` - Finds the main repository for a worktree (src/git/utils.rs:40)
- Worktree init creates symlinks to main repository's `.thoughts-data` (src/commands/init.rs:16-57)
- No duplicate FUSE mounts or manual cleanup required

### Three-Space Architecture
- **Thoughts Space**: Single git repository for personal work, organized by branch
- **Context Space**: Multiple team-shared repositories, each in its own subdirectory
- **References Space**: Read-only external repositories, auto-organized by org/repo

### Auto-Mount System
- Automatic mount management for all three spaces
- Mount path resolution through `MountResolver`
- Read-only enforcement for reference mounts
- Unique target keys prevent mount conflicts

### Work Organization
- Branch-based directories for feature work
- Main/master branches are locked down (must create feature branch first)
- Legacy weekly directories (YYYY-WWW) auto-archived to completed/ on first run
- Automatic directory structure with research/, plans/, artifacts/
- Work completion moves to dated directories

### Repository Mapping System
- Maps repository URLs to local filesystem paths
- Supports multiple repository locations
- Critical for tracking repository locations across the system
- Managed by `RepoMappingManager`

### Type-Safe Mount Identification
- `MountSpace` enum provides compile-time safety
- Pattern matching ensures exhaustive handling
- Automatic org/repo extraction for references
- Clean separation between CLI strings and internal types

## Manual Verification Checklist — Branch Protection & Weekly Auto-Archive

1. Run `thoughts work init` on main → should fail with standardized message
2. Run `thoughts work init` on feature branch → should succeed
3. Run `thoughts work complete` on main → should fail with standardized message
4. Run `thoughts work list` on main → should succeed (branch-agnostic)
5. MCP write_document on main → should fail with standardized message
6. MCP list_active_documents on main → should fail with standardized message
7. Weekly directories (2025-W01) are auto-archived to completed/

<!-- BEGIN:xtask:autogen header -->
- Crate: thoughts-tool
- Path: crates/infra/thoughts-core/
- Role: lib
- Family: infra
- Integrations: mcp=true, logging=true, napi=false
<!-- END:xtask:autogen -->

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p thoughts-tool -- --check
cargo clippy -p thoughts-tool --all-targets -- -D warnings

# Tests
cargo test -p thoughts-tool

# Build
cargo build -p thoughts-tool
```
<!-- END:xtask:autogen -->
