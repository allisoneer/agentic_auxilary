# CLAUDE.md - agentic-config

<!-- BEGIN:xtask:autogen header -->
- Crate: agentic-config
- Path: crates/infra/agentic-config/
- Role: lib
- Family: infra
- Integrations: mcp=false, logging=false, napi=false
<!-- END:xtask:autogen -->

## Overview

Unified configuration system for the agentic tools ecosystem. Handles loading, merging, and validating `agentic.toml` configuration files with support for global/local precedence, environment variable overrides, and advisory warnings.

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

1. **Global**: `~/.config/agentic/agentic.toml` (user-wide defaults)
2. **Local**: `./agentic.toml` (per-project overrides)
3. **Environment variables**: Override any value (highest precedence)

The loader (`loader::load_merged()`) performs a TOML deep-merge of global into local, then applies env var overrides.

## Tool-Specific Config Sections

### `subagents` - Coding Agent Tools
Model selection for `ask_agent` tool subagents:
- `locator_model`: Fast discovery agent (default: `claude-haiku-4-5`)
- `analyzer_model`: Deep analysis agent (default: `claude-sonnet-4-6`)
- `runtime_timeout_secs`: ask_agent wall-clock timeout in seconds (default: `3600`, `0 = disabled`)

### `reasoning` - GPT-5 Reasoner
Model selection for `ask_reasoning_model` tool:
- `optimizer_model`: Prompt optimizer (default: `anthropic/claude-sonnet-4.6`)
- `executor_model`: Reasoning executor (default: `openai/gpt-5.2`)
- `reasoning_effort`: Optional effort level (`low`, `medium`, `high`, `xhigh`)

### `services` - External APIs
Base URLs and timeout knobs for Anthropic, Exa, Linear, and GitHub integrations. API keys are loaded from environment only (never serialized).

### `review`

- `run_timeout_secs`: review_run wall-clock timeout in seconds (default: `1800`, `0 = disabled`)

### `thoughts`

- `add_reference_timeout_secs`: thoughts_add_reference wall-clock timeout in seconds (default: `600`, `0 = disabled`)

### `logging` - Diagnostics
Log level and JSON formatting preferences.

## Environment Variable Overrides

| Variable | Config Path |
|----------|-------------|
| `AGENTIC_SUBAGENTS_LOCATOR_MODEL` | `subagents.locator_model` |
| `AGENTIC_SUBAGENTS_ANALYZER_MODEL` | `subagents.analyzer_model` |
| `AGENTIC_SUBAGENTS_RUNTIME_TIMEOUT_SECS` | `subagents.runtime_timeout_secs` |
| `AGENTIC_REASONING_OPTIMIZER_MODEL` | `reasoning.optimizer_model` |
| `AGENTIC_REASONING_EXECUTOR_MODEL` | `reasoning.executor_model` |
| `AGENTIC_REASONING_EFFORT` | `reasoning.reasoning_effort` |
| `AGENTIC_CLI_TOOLS_JUST_EXECUTE_TIMEOUT_SECS` | `cli_tools.just_execute_timeout_secs` |
| `AGENTIC_CLI_TOOLS_JUST_SEARCH_TIMEOUT_SECS` | `cli_tools.just_search_timeout_secs` |
| `AGENTIC_SERVICES_LINEAR_BASE_URL` | `services.linear.base_url` |
| `AGENTIC_SERVICES_LINEAR_CONNECT_TIMEOUT_SECS` | `services.linear.connect_timeout_secs` |
| `AGENTIC_SERVICES_LINEAR_REQUEST_TIMEOUT_SECS` | `services.linear.request_timeout_secs` |
| `AGENTIC_SERVICES_GITHUB_BASE_URL` | `services.github.base_url` |
| `AGENTIC_SERVICES_GITHUB_TOTAL_TIMEOUT_SECS` | `services.github.total_timeout_secs` |
| `AGENTIC_REVIEW_RUN_TIMEOUT_SECS` | `review.run_timeout_secs` |
| `AGENTIC_THOUGHTS_ADD_REFERENCE_TIMEOUT_SECS` | `thoughts.add_reference_timeout_secs` |

## Deprecations and Warnings

The system uses advisory warnings (non-fatal) for:
- **Invalid values**: Empty strings, format mismatches, suspicious executor models
- **Reasoning effort**: Invalid enum values warn but don't fail

Warnings are returned via `LoadedAgenticConfig.warnings` and printed by CLI commands.

## Module Structure

- `types.rs`: Config structs and serialization
- `paths.rs`: XDG path resolution and config directory helpers
- `loader.rs`: Load, merge, and env override logic
- `merge.rs`: TOML deep-merge implementation
- `validation.rs`: Advisory validation and deprecated key detection
- `schema.rs`: JSON schema generation
- `test_support.rs`: Test-only env guards (crate-private)

## Adding a New Config Section

Follow the pattern of `subagents` and `reasoning`:

1. Add struct to `types.rs` with `#[derive(Default, Serialize, Deserialize, JsonSchema)]`
2. Add field to `AgenticConfig` with `#[serde(default)]`
3. Add env var overrides in `loader.rs` `apply_env_overrides()`
4. Add validation rules in `validation.rs` `validate()`
5. Thread config to consumers via `AgenticToolsConfig` in registry

## Example agentic.toml

```toml
"$schema" = "file://./agentic.schema.json"

[subagents]
locator_model = "claude-haiku-4-5"
analyzer_model = "claude-sonnet-4-6"
runtime_timeout_secs = 3600

[reasoning]
optimizer_model = "anthropic/claude-sonnet-4.6"
executor_model = "openai/gpt-5.2"
reasoning_effort = "high"

[services.anthropic]
base_url = "https://api.anthropic.com"

[services.linear]
base_url = "https://api.linear.app/graphql"
connect_timeout_secs = 10
request_timeout_secs = 60

[services.github]
base_url = "https://api.github.com"
total_timeout_secs = 120

[review]
run_timeout_secs = 1800

[thoughts]
add_reference_timeout_secs = 600

[logging]
level = "info"
json = false
```

## Notes

Add any human-authored notes below. Content outside autogen blocks is preserved by xtask sync.
