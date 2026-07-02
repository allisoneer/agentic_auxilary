# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.7.1] - 2026-07-02

### 🐛 Bug Fixes
- *(xtask)* Generate crate claude commands with just wrappers
## [0.7.0] - 2026-06-18

### ⛰️  Features
- *(workspace_tools)* Add workspace-scoped mcp file and todo tools
## [0.6.0] - 2026-06-05

### ⛰️  Features
- *(opencode-orchestrator-mcp)* Add explicit agent listing and selection
## [0.5.0] - 2026-05-20

### ⛰️  Features
- Add configurable runtime timeouts across agentic tools

### 🐛 Bug Fixes
- *(logging)* Address PR #220 timeout and failure handling review fixes
## [0.4.0] - 2026-05-19

### ⛰️  Features
- *(opencode-orchestrator-mcp)* Add command allow and deny filtering
## [0.3.0] - 2026-05-04

### 🐛 Bug Fixes
- *(gpt5_reasoner)* Raise completion headroom and log executor diagnostics
- [**breaking**] Remove legacy reasoning compatibility paths
- *(agentic-config)* [**breaking**] Removed support for deprecated `reasoning.token_limit` (TOML) and `AGENTIC_REASONING_TOKEN_LIMIT` (env). Use `reasoning.max_input_tokens` / `AGENTIC_REASONING_MAX_INPUT_TOKENS` only (no aliasing or deprecation warnings).
## [0.2.0] - 2026-05-03

### ⛰️  Features
- *(gpt5_reasoner)* Stream executor responses with recovery
## [0.1.1] - 2026-03-27

### ⚙️  Miscellaneous
- Release
## [0.1.0] - 2026-03-26

### ⚙️  Miscellaneous
- *(config)* Remove JSON-era code remnants from config system
- *(build)* Enable nightly rustfmt and standardize workspace lints
- Resolve PR #127 review feedback and enable taplo verification

### ⛰️  Features
- *(thoughts_tool)* [**breaking**] Rewrite configuration system with unified agentic-config
- *(agentic-config)* [**breaking**] Replace ModelsConfig with tool-specific subagents and reasoning sections
- *(config)* [**breaking**] Rewrite configuration system from JSON to TOML

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints

### 🐛 Bug Fixes
- *(thoughts_tool)* Eliminate TOCTOU races in repos.json migration and load
- *(agentic-config)* Narrow TOCTOU window in legacy config migration
- Add Unix-only compile guards to all Unix-dependent crates
- *(agentic-config)* Isolate loader tests from real global config
- *(agentic-config)* Address PR #124 v6 review comments
- *(config)* Address follow-up issues from TOML rewrite

### 📚 Documentation
- Write comprehensive CLAUDE.md for agentic-config and agentic-bin
- *(config)* Add PR #127 groups 5, 7, 10, 11 TODO comments

### 🚜 Refactor
- *(agentic-config)* Return Value directly from infallible mapping function
- *(agentic-config)* Make loader read-only with in-memory legacy fallback
- *(agentic-config)* Remove premature models deprecation handling
- *(config)* Remove unused service configs and wire up base_url fields

### 🧪 Testing
- *(agentic-config)* Add EnvGuard isolation to remaining loader tests
