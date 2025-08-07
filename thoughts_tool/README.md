# Thoughts Tool

A flexible thought management tool that helps developers organize notes and documentation across git repositories using filesystem mounts (mergerfs on Linux, fuse-t on macOS).

## What is Thoughts Tool?

Thoughts Tool creates a unified filesystem view of documentation scattered across multiple git repositories. It automatically mounts and syncs git-backed directories, allowing you to access all your project notes, decisions, and documentation from a single location while keeping them versioned with their respective codebases.

## Key Features

- 🔀 **Unified Filesystem**: Merge documentation from multiple repositories into a single mountpoint
- 🔄 **Automatic Git Sync**: Keep your thoughts synchronized across repositories
- 🖥️ **Cross-Platform**: Works on Linux (mergerfs) and macOS (fuse-t)
- 📁 **Flexible Organization**: Support for collections, patterns, and custom mount configurations
- 🔧 **Repository Integration**: Seamlessly integrates with existing git workflows
- 🎯 **Worktree Support**: Full support for git worktrees
- 🚀 **Auto-Mount System**: Automatic mount management for configured repositories

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

This creates a `.thoughts/` directory in your repository to store thoughts and documentation.

### 2. Add a Mount

```bash
# Add a git-backed mount
thoughts mount add https://github.com/user/docs-repo.git docs

# Or add a local directory mount
thoughts mount add /path/to/local/docs local-docs
```

### 3. Check Status

```bash
thoughts status
```

### 4. Sync Your Mounts

```bash
thoughts sync
```

## Usage

### Command Structure

```bash
thoughts [COMMAND] [OPTIONS]
```

### Available Commands

#### Core Commands
- `init` - Initialize thoughts for a repository
- `sync` - Sync all git-backed mounts
- `status` - Show current mount status and configuration

#### Mount Management
- `mount add <source> <name>` - Add a new mount
- `mount remove <name>` - Remove a mount
- `mount list` - List all configured mounts
- `mount update <name>` - Update mount configuration
- `mount clone <source> <name>` - Clone and add a repository as a mount

#### Configuration Management
- `config create` - Create a new configuration
- `config show` - Display current configuration
- `config edit` - Edit configuration with $EDITOR
- `config validate` - Validate configuration syntax

## Configuration

Thoughts Tool uses a three-tier configuration system:

### 1. Personal Configuration
User-wide settings stored in `~/.config/thoughts/`

### 2. Repository Configuration  
Project-specific settings in `.thoughts/config.json`

### 3. Merged Configuration
Runtime combination of personal and repository configurations

### Configuration Example

```json
{
  "mounts": [
    {
      "name": "shared-docs",
      "source": "https://github.com/team/shared-docs.git",
      "mount_point": "docs/shared",
      "sync_strategy": "auto",
      "patterns": ["*.md", "*.txt"]
    }
  ],
  "collections": {
    "project-docs": {
      "description": "Project documentation collection",
      "mounts": ["shared-docs", "local-notes"]
    }
  }
}
```

## Architecture

### Platform Abstraction
The tool automatically detects your platform and uses the appropriate mount technology:
- **Linux**: Uses mergerfs for high-performance union filesystem
- **macOS**: Uses fuse-t for FUSE support on Apple Silicon and Intel Macs

### Mount Resolution
1. Checks for user-managed repository clones
2. Falls back to auto-managed clones in `~/.thoughts/mounts/`
3. Resolves patterns and collections
4. Merges configurations based on rules

### Git Integration
- Full support for worktrees
- Automatic detection of repository boundaries
- Smart sync strategies (auto, manual, on-demand)
- Conflict resolution helpers

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

### Collections
Group related mounts together for easier management:

```bash
# Work with all mounts in a collection
thoughts sync --collection project-docs
```

### Patterns
Use glob patterns to selectively sync files:

```json
{
  "patterns": ["*.md", "docs/**/*.txt", "!*.tmp"]
}
```

### Rules Framework
Define validation rules and metadata for your thoughts:

```json
{
  "rules": {
    "require_frontmatter": true,
    "max_file_size": "10MB",
    "allowed_extensions": [".md", ".txt", ".adoc"]
  }
}
```

### Auto-Mount System
Configure repositories to automatically mount when accessed:

```json
{
  "auto_mount": {
    "enabled": true,
    "patterns": ["github.com/myorg/*"],
    "mount_point_template": "external/{repo_name}"
  }
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