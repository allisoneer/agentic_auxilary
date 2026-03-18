# CLAUDE.md - agentic-bin

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-bin
- Path: apps/agentic/
- Role: app
- Family: tools
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

CLI application for managing `agentic.toml` configuration files. Provides commands for creating, viewing, editing, and validating configuration with support for global/local config precedence and advisory warnings.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p agentic-bin -- --check
cargo clippy -p agentic-bin --all-targets -- -D warnings

# Tests
cargo test -p agentic-bin

# Build
cargo build -p agentic-bin
```
<!-- END:xtask:autogen -->

## CLI Subcommands

### `config init [--global] [--force]`
Create a new `agentic.toml` with default values.
- `--global`: Create in `~/.config/agentic/` instead of current directory
- `--force`: Overwrite existing file

### `config show [--json] [--path PATH]`
Display the merged configuration (global + local + env overrides).
- `--json`: Output raw JSON (default is pretty-printed)
- `--path`: Use specified directory instead of current directory

Also prints any migration events and advisory warnings.

### `config schema`
Output the JSON Schema for `agentic.toml`. Useful for IDE autocomplete setup.

```bash
agentic config schema > agentic.schema.json
```

### `config edit [--global]`
Open config in `$EDITOR` with post-edit validation.
- `--global`: Edit global config instead of local
- Creates config with defaults if it doesn't exist
- Validates after save and shows any warnings

### `config validate [--path PATH]`
Check configuration for errors and warnings without modifying.
- Shows advisory warnings for deprecated keys, invalid values, etc.
- Exit code 0 even with warnings (non-fatal)

## How Config Loading Works

The CLI uses `agentic_config::loader::load_merged()` which:
1. Finds global config at `~/.config/agentic/agentic.toml`
2. Finds local config at `./agentic.toml` (or `--path`)
3. Performs TOML deep-merge (local wins)
4. Applies environment variable overrides
5. Runs validation and collects warnings

## Common Workflows

### Initial Setup
```bash
# Create global defaults
agentic config init --global

# Create project-specific overrides
agentic config init
```

### Check Current Configuration
```bash
# See merged config with warnings
agentic config show

# Validate without changing anything
agentic config validate
```

### Configure Models
```bash
# Edit config in your editor
agentic config edit

# Or set via environment
export AGENTIC_SUBAGENTS_LOCATOR_MODEL=claude-haiku-4-5
export AGENTIC_REASONING_OPTIMIZER_MODEL=anthropic/claude-sonnet-4.6
```

## Troubleshooting

### Warnings vs Errors
- **Warnings**: Advisory only, config still loads (e.g., deprecated keys, suspicious values)
- **Errors**: Config cannot be parsed (e.g., invalid TOML syntax)

### Common Warnings
- `invalid_value: reasoning.optimizer_model`: Should use OpenRouter format (`provider/model`)
- `invalid_value: reasoning.reasoning_effort`: Must be `low`, `medium`, `high`, or `xhigh`

## Module Structure

- `commands/config.rs`: All config subcommand implementations
- `commands/mod.rs`: Command routing
- `main.rs`: CLI entry point with clap

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
