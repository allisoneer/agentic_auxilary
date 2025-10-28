# Thoughts Tool v2

A flexible thought management tool that helps developers organize notes and documentation across git repositories using filesystem mounts (mergerfs on Linux, fuse-t on macOS) with a three-space architecture.

## What is Thoughts Tool?

Thoughts Tool creates a unified filesystem view of documentation through three distinct mount spaces:
- **thoughts/** - Your personal workspace for work documents, plans, and research
- **context/** - Team-shared documentation and context repositories
- **references/** - Read-only external code repositories for reference

It automatically mounts and syncs git-backed directories, allowing you to access all your project notes, decisions, and documentation from a single location while keeping them versioned with their respective repositories.

## Key Features

- 🗂️ **Three-Space Architecture**: Organized separation of thoughts, context, and references
- 🔄 **Automatic Git Sync**: Keep your documentation synchronized across repositories
- 🖥️ **Cross-Platform**: Works on Linux (mergerfs) and macOS (fuse-t)
- 📚 **Reference Management**: Read-only mounts for external code repositories
- 🌿 **Branch-Based Work Organization**: Automatic directory structure based on current branch/week
- 🔧 **Repository Integration**: Seamlessly integrates with existing git workflows
- 🎯 **Worktree Support**: Full support for git worktrees
- 🚀 **Auto-Mount System**: Automatic mount management for all three spaces

## Installation

### Prerequisites

#### Linux
- mergerfs installed (`apt install mergerfs` or `yum install mergerfs`)
- FUSE support enabled
- Git installed

#### macOS
- fuse-t installed (`brew install macos-fuse-t/homebrew-cask/fuse-t`)
- Git installed

### Building from Source

```bash
# Clone the repository
git clone <repository-url>
cd thoughts_tool

# Build the project
make build

# Or build with release optimizations
make release

# Install globally
make install
```

## Quick Start

### 1. Initialize Thoughts for Your Repository

```bash
cd /path/to/your/project
thoughts init
```

This creates:
- `.thoughts/` directory for configuration
- `.thoughts-data/` directory for mount storage
- Three symlinks: `thoughts/`, `context/`, and `references/` pointing to the mount spaces

### 2. Configure Your Thoughts Mount (optional)

Edit `.thoughts/config.json` to add your personal workspace repository:

```json
{
  "version": "2.0",
  "thoughts_mount": {
    "remote": "git@github.com:user/my-thoughts.git",
    "sync": "auto"
  }
}
```

### 3. Add Context Mounts

```bash
# Add a team documentation repository
thoughts mount add https://github.com/team/docs-repo.git team-docs
```

### 4. Add Reference Repositories

```bash
# Add a reference repository (automatically organized by org/repo)
thoughts references add https://github.com/rust-lang/rust
```

### 5. Update All Mounts

```bash
# Mount everything configured
thoughts mount update

# Sync all git repositories
thoughts sync --all
```

### 6. Start Working

```bash
# Initialize a work directory for current branch/week
thoughts work init
```

## Usage

### Command Structure

```bash
thoughts [COMMAND] [OPTIONS]
```

### Available Commands

#### Core Commands
- `init` - Initialize thoughts for a repository
- `sync [<mount>]` - Sync specific mount or all with --all
- `status` - Show current mount status and configuration

#### Mount Management (Context Mounts)
- `mount add <source> <name>` - Add a new context mount
- `mount remove <name>` - Remove a context mount
- `mount list` - List all configured mounts
- `mount update` - Update/refresh all active mounts
- `mount clone <url> [<path>]` - Clone a repository to local path

#### Reference Management
- `references add <url>` - Add a reference repository
- `references remove <url>` - Remove a reference repository
- `references list` - List all configured references
- `references sync` - Clone missing reference repositories

#### Work Management
- `work init` - Initialize work directory for current branch/week
- `work complete` - Move current work to completed with date range
- `work list [--recent N]` - List active and completed work directories

#### Configuration Management
- `config create` - Create a new configuration
- `config show` - Display current configuration
- `config edit` - Edit configuration with $EDITOR
- `config validate` - Validate configuration syntax

## Configuration

Thoughts Tool uses a repository-based configuration system with automatic v1 to v2 migration support.

### Configuration Structure

The configuration file (`.thoughts/config.json`) defines:
- **thoughts_mount** - Your personal workspace repository (optional)
- **context_mounts** - Team-shared documentation repositories
- **references** - External code repositories for reference
- **mount_dirs** - Directory names for the three spaces (defaults: thoughts, context, references)

### v2 Configuration Example

```json
{
  "version": "2.0",
  "mount_dirs": {
    "thoughts": "thoughts",
    "context": "context",
    "references": "references"
  },
  "thoughts_mount": {
    "remote": "git@github.com:user/my-thoughts.git",
    "subpath": "projects/current",
    "sync": "auto"
  },
  "context_mounts": [
    {
      "remote": "https://github.com/team/shared-docs.git",
      "mount_path": "team-docs",
      "sync": "auto"
    },
    {
      "remote": "git@github.com:company/architecture.git",
      "mount_path": "architecture",
      "subpath": "docs",
      "sync": "auto"
    }
  ],
  "references": [
    "https://github.com/rust-lang/rust",
    "git@github.com:tokio-rs/tokio.git"
  ]
}
```

### Migration from v1

**Automatic migration happens on the first write operation** (e.g., `thoughts init`, `thoughts mount add`):

- V1 configs are automatically converted to v2 format
- A timestamped backup is created if you have non-empty mounts or rules (`.thoughts/config.v1.bak-*.json`)
- Migration rules:
  - Mounts with `sync: none` or paths starting with `references/` → become references
  - Other mounts → become context mounts
  - Rules field → dropped (preserved in backup only)
- One-line message confirms migration with link to full guide

You can also explicitly migrate with:
```bash
thoughts config migrate-to-v2 --dry-run  # Preview
thoughts config migrate-to-v2 --yes      # Execute
```

For detailed migration instructions, see [MIGRATION_V1_TO_V2.md](./MIGRATION_V1_TO_V2.md).

## Architecture

### Three-Space Design
The tool organizes all mounts into three distinct spaces:

1. **Thoughts Space** (`thoughts/`)
   - Single git repository for personal work
   - Organized by branch/week in `active/` and `completed/` directories
   - Supports subpath mounting for monorepo scenarios

2. **Context Space** (`context/`)
   - Multiple team-shared repositories
   - Each mount gets its own subdirectory
   - Full read-write access for collaboration

3. **References Space** (`references/`)
   - Read-only external code repositories
   - Auto-organized by `{org}/{repo}` structure
   - Never synced to prevent accidental modifications

### Platform Abstraction
The tool automatically detects your platform and uses the appropriate mount technology:
- **Linux**: Uses mergerfs for high-performance union filesystem
- **macOS**: Uses fuse-t for FUSE support on Apple Silicon and Intel Macs

### Mount Resolution
1. Uses type-safe `MountSpace` enum for mount identification
2. Resolves to unique paths under `.thoughts-data/`
3. Handles automatic cloning for missing repositories
4. Maintains mappings in `~/.thoughts/repos.json`

### Git Integration
- Full support for worktrees (see Git Worktree Support section)
- Automatic detection of repository boundaries
- Smart sync strategies (auto for thoughts/context, none for references)
- Branch-based work organization

## Git Worktree Support

thoughts_tool automatically detects and handles git worktrees. When you run `thoughts init` in a worktree:

1. It detects you're in a worktree
2. Verifies the main repository is initialized
3. Creates a symlink to share the main repository's mounts
4. No duplicate FUSE mounts are created

### Usage

```bash
# Initialize main repository first
cd /path/to/main/repo
thoughts init

# Create a worktree
git worktree add ../my-feature-branch

# Initialize the worktree (shares main repo's mounts)
cd ../my-feature-branch
thoughts init
```

### How It Works

Worktrees use a simple symlink approach:
- `.thoughts-data` -> Points to main repository's `.thoughts-data`
- The `thoughts`, `context`, and `references` symlinks are already tracked in git

This ensures:
- No duplicate mounts
- Consistent access to all three spaces across worktrees
- Automatic cleanup when worktree is removed

## Development

### Building and Testing

```bash
# Run all checks, tests, and build
make all

# Run specific components
make check      # Run clippy
make test       # Run tests
make build      # Build the project

# Run with verbose output
make check-verbose
make test-verbose
make build-verbose

# Run specific test types
make test-unit          # Unit tests only
make test-integration   # Integration tests (requires THOUGHTS_INTEGRATION_TESTS=1)

# Format code
make fmt

# Check dependencies
make audit      # Security audit
make outdated   # Check for outdated dependencies
```

### Project Structure

```
thoughts_tool/
├── src/
│   ├── commands/      # CLI command implementations
│   ├── config/        # Configuration management
│   ├── git/          # Git integration
│   ├── mount/        # Mount implementations
│   ├── platform/     # Platform detection and abstraction
│   └── utils/        # Utility functions
├── tests/            # Integration tests
├── hack/            # Development utilities
└── Makefile         # Build automation
```

## Advanced Features

### Work Organization

The work commands help organize your documentation by branch or week:

```bash
# On feature branch - creates thoughts/active/my-feature/
thoughts work init

# On main branch - creates thoughts/active/2025_week_04/
thoughts work init

# Complete work - moves to thoughts/completed/2025-01-15_to_2025-01-22_my-feature/
thoughts work complete
```

Each work directory includes:
- `research/` - Investigation notes and findings
- `plans/` - Design documents and implementation plans
- `artifacts/` - Generated files, diagrams, exports
- `manifest.json` - Metadata about the work session

### Reference Repository Management

References are read-only external repositories organized by org/repo:

```bash
# Add multiple references
thoughts references add https://github.com/rust-lang/rust
thoughts references add https://github.com/tokio-rs/tokio

# They mount to:
# references/rust-lang/rust/
# references/tokio-rs/tokio/

# Sync all references (clones if missing)
thoughts references sync
```

### Subpath Mounting

Mount specific subdirectories from larger repositories:

```json
{
  "thoughts_mount": {
    "remote": "git@github.com:user/monorepo.git",
    "subpath": "projects/current",
    "sync": "auto"
  },
  "context_mounts": [{
    "remote": "git@github.com:company/docs.git",
    "mount_path": "api-docs",
    "subpath": "api/v2",
    "sync": "auto"
  }]
}
```

## Troubleshooting

### Mount Permission Issues
If you encounter permission errors:
1. Ensure FUSE is properly installed and configured
2. Check that your user has permission to mount filesystems
3. On Linux, you may need to add your user to the `fuse` group

### Platform Detection Failed
The tool will inform you if required mount utilities are missing:
- Linux: Install mergerfs
- macOS: Install fuse-t via Homebrew

### Git Sync Conflicts
When sync conflicts occur:
1. The tool will notify you of conflicts
2. Resolve conflicts in the affected repository
3. Run `thoughts sync` again

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

### Development Setup

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `make test-all`
5. Format code: `make fmt`
6. Submit a pull request

## License

MIT - See [LICENSE](../LICENSE) in the root of the repository.

## Acknowledgments

Built with excellent Rust libraries:
- [clap](https://github.com/clap-rs/clap) for CLI parsing
- [git2](https://github.com/rust-lang/git2-rs) for Git operations
- [serde](https://github.com/serde-rs/serde) for serialization
- [tokio](https://github.com/tokio-rs/tokio) for async runtime
- [tracing](https://github.com/tokio-rs/tracing) for structured logging