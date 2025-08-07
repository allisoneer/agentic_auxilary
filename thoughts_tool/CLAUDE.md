# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust CLI application called "thoughts" - a flexible thought management tool that helps developers organize notes and documentation across git repositories using filesystem mounts (mergerfs on Linux, fuse-t on macOS).

## Common Development Commands

### Building and Testing
```bash
# Default targets (silent if succeasful)
make check      # Run clippy
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
- `init` - Initialize thoughts for a repository
- `sync` - Sync git-backed mounts
- `status` - Show mount status
- `mount/*` - Mount management (add, remove, list, update, clone)
- `config/*` - Configuration management (create, show, edit, validate)
  - Note: `get` and `set` commands exist in the codebase but are not wired to the CLI

### Platform Abstraction
The application supports both Linux (mergerfs) and macOS (fuse-t) through:
- Platform detection at runtime (`/src/platform/detector.rs`)
- Strategy pattern for mount implementations (`/src/mount/`)
- Conditional compilation with feature flags

### Configuration System
- JSON-based configuration managed by `ConfigManager`
- Schema validation and enforcement via `MountValidator`
- Interactive editing with `$EDITOR`
- Collections support for organizing mounts
- Three-tier configuration hierarchy (personal, repository, merged)
- Repository mapping system (`RepoMappingManager`) for URL to local path mappings
- Personal configuration management (`PersonalManager`) for user-wide settings
- Mount configuration merging (`MountMerger`) for combining configurations
- Rules framework for file metadata and validation

### Testing Strategy
- Unit tests embedded in modules (`#[cfg(test)]`)
- Integration tests in `/tests/` directory requiring `THOUGHTS_INTEGRATION_TESTS=1`
- Mock implementations for testing mount operations
- Temporary directories for filesystem testing

## Key Implementation Notes

1. **Rust Edition**: Uses Rust 2024 edition - ensure compatibility when adding dependencies
2. **Error Handling**: Uses `anyhow` for application errors and `thiserror` for library errors
3. **Async Runtime**: Uses `tokio` for async operations with full features
4. **Git Integration**: Uses `git2` crate for repository operations
5. **Platform Features**: Platform-specific code uses conditional compilation (`#[cfg(target_os = "linux")]`)
6. **Build Warnings**: Currently suppresses warnings in silent mode (TODO: fix all warnings)

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
- **git2** - Git repository operations

## Advanced Features

### Git Worktree Support

The tool now fully supports git worktrees through automatic detection and smart initialization:

- `is_worktree()` - Detects if current directory is a worktree (src/git/utils.rs:25)
- `get_main_repo_for_worktree()` - Finds the main repository for a worktree (src/git/utils.rs:40)
- Worktree init creates symlinks to main repository's `.thoughts-data` (src/commands/init.rs:16-57)
- No duplicate FUSE mounts or manual cleanup required

### Auto-Mount System
- Automatic mount management via `AutoMountManager`
- Mount path resolution through `MountResolver`
- Distinguishes between auto-managed and user-managed repository clones
- Handles mount patterns and repository-specific configurations

### Repository Mapping System
- Maps repository URLs to local filesystem paths
- Supports multiple repository locations
- Critical for tracking repository locations across the system
- Managed by `RepoMappingManager`

### Validation Framework
- Comprehensive configuration validation
- Ensures mount configurations are valid
- Prevents configuration conflicts
- Enforces rules and constraints
