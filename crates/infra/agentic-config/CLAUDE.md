# CLAUDE.md - agentic-config

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-config
- Path: crates/infra/agentic-config/
- Role: lib
- Family: infra
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Unified configuration system for the agentic tools ecosystem. Handles loading, merging, and validating `agentic.json` configuration files with support for global/local precedence, environment variable overrides, and advisory warnings.

## Quick Commands

<!-- BEGIN:xtask:autogen commands -->
```bash
# Lint & Clippy
cargo fmt -p agentic-config -- --check
cargo clippy -p agentic-config --all-targets -- -D warnings

# Tests
cargo test -p agentic-config

# Build
cargo build -p agentic-config
```
<!-- END:xtask:autogen -->

## Configuration Files and Precedence

Configuration is loaded from two locations and merged (local wins):

1. **Global**: `~/.config/agentic/agentic.json` (user-wide defaults)
2. **Local**: `./agentic.json` (per-project overrides)
3. **Environment variables**: Override any value (highest precedence)

The loader (`loader::load_merged()`) performs a JSON merge-patch of global into local, then applies env var overrides.

## Tool-Specific Config Sections

### `thoughts` - Workspace Configuration
Configures the three-space thoughts architecture (thoughts/context/references mounts).

### `subagents` - Coding Agent Tools
Model selection for `ask_agent` tool subagents:
- `locator_model`: Fast discovery agent (default: `claude-haiku-4-5`)
- `analyzer_model`: Deep analysis agent (default: `claude-sonnet-4-6`)

### `reasoning` - GPT-5 Reasoner
Model selection for `ask_reasoning_model` tool:
- `optimizer_model`: Prompt optimizer (default: `anthropic/claude-sonnet-4.6`)
- `executor_model`: Reasoning executor (default: `openai/gpt-5.2`)
- `reasoning_effort`: Optional effort level (`low`, `medium`, `high`, `xhigh`)

### `services` - External APIs
Base URLs for Anthropic and Exa APIs. API keys are loaded from environment only (never serialized).

### `logging` - Diagnostics
Log level and JSON formatting preferences.

## Environment Variable Overrides

| Variable | Config Path |
|----------|-------------|
| `AGENTIC_SUBAGENTS_LOCATOR_MODEL` | `subagents.locator_model` |
| `AGENTIC_SUBAGENTS_ANALYZER_MODEL` | `subagents.analyzer_model` |
| `AGENTIC_REASONING_OPTIMIZER_MODEL` | `reasoning.optimizer_model` |
| `AGENTIC_REASONING_EXECUTOR_MODEL` | `reasoning.executor_model` |
| `AGENTIC_REASONING_EFFORT` | `reasoning.reasoning_effort` |

## Deprecations and Warnings

The system uses advisory warnings (non-fatal) for:
- **Deprecated keys**: Old `models` config section is detected and warns
- **Invalid values**: Empty strings, format mismatches, suspicious executor models
- **Reasoning effort**: Invalid enum values warn but don't fail

Warnings are returned via `LoadedAgenticConfig.warnings` and printed by CLI commands.

## Module Structure

- `types.rs`: All config structs with `#[serde(default)]` for partial configs
- `loader.rs`: `load_merged()` function, env var overrides, path resolution
- `validation.rs`: `validate()` and `detect_deprecated_keys()` functions
- `lib.rs`: Public exports

## Adding a New Config Section

Follow the pattern of `subagents` and `reasoning`:

1. Add struct to `types.rs` with `#[derive(Default, Serialize, Deserialize, JsonSchema)]`
2. Add field to `AgenticConfig` with `#[serde(default)]`
3. Add env var overrides in `loader.rs` `apply_env_overrides()`
4. Add validation rules in `validation.rs` `validate()`
5. Thread config to consumers via `AgenticToolsConfig` in registry

## Example agentic.json

```json
{
  "$schema": "https://example.com/agentic.schema.json",
  "thoughts": {
    "mount_dirs": {
      "thoughts": "thoughts",
      "context": "context",
      "references": "references"
    }
  },
  "subagents": {
    "locator_model": "claude-haiku-4-5",
    "analyzer_model": "claude-sonnet-4-6"
  },
  "reasoning": {
    "optimizer_model": "anthropic/claude-sonnet-4.6",
    "executor_model": "openai/gpt-5.2",
    "reasoning_effort": "high"
  },
  "services": {
    "anthropic": {
      "base_url": "https://api.anthropic.com"
    }
  },
  "logging": {
    "level": "info",
    "json": false
  }
}
```

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
