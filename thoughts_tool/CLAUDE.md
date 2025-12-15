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
# Default targets (silent if succeasful)
make check      # Run formatting and clippy checks
make test       # Run all tests
make build      # Build the project

# Normal output versions
make check-normal
make test-normal
make build-normal

# Verbose versions
make check-verbose
make test-verbose
make build-verbose

# Specific test types
make test-unit          # Unit tests only
make test-integration   # Integration tests (requires THOUGHTS_INTEGRATION_TESTS=1)
make test-all          # All tests with all features

# Other useful commands
make fmt        # Format code
make fmt-check  # Check formatting

# Development and Maintenance
make clean      # Clean build artifacts
make doc        # Build and open documentation
make audit      # Security vulnerability check
make outdated   # Check outdated dependencies

# Build Commands
make release    # Release build
make install    # Install the application
make run        # Run the application

# Meta Commands
make all        # Default target running check, test, build
make help       # Show all available targets with descriptions
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
THOUGHTS_INTEGRATION_TESTS=1 THOUGHTS_NETWORK_TESTS=1 make test
```

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
- **Thoughts Space**: Single git repository for personal work, organized by branch/week
- **Context Space**: Multiple team-shared repositories, each in its own subdirectory
- **References Space**: Read-only external repositories, auto-organized by org/repo

### Auto-Mount System
- Automatic mount management for all three spaces
- Mount path resolution through `MountResolver`
- Read-only enforcement for reference mounts
- Unique target keys prevent mount conflicts

### Work Organization
- Branch-based directories for feature work
- ISO week-based directories for main branch work
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
