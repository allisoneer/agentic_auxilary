# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.11.0] - 2026-04-06

### ⛰️  Features
- *(thoughts-tool)* Improve sync reliability and add shareable URL generation

### 🐛 Bug Fixes
- *(thoughts-tool)* Normalize repo_subpath in GitHub URL composition
- *(thoughts-tool)* Tighten tool log matching and correct divergence analysis
- *(thoughts-tool)* Guard against empty org_path in GitHub URL composition
- *(thoughts-tool)* Correct github url ref handling and path encoding
- *(thoughts-tool)* Write merged JSONL to working tree during conflict resolution
- *(thoughts-tool)* Set bare repo HEAD for clone compatibility in test
- *(thoughts-tool)* Use add_path instead of add_frombuffer for conflict resolution

### 🧪 Testing
- *(thoughts-tool)* Add divergence and JSONL smart-merge coverage
## [0.10.0] - 2026-03-26

### ⚙️  Miscellaneous
- Resolve PR #127 review feedback and enable taplo verification

### ⛰️  Features
- *(thoughts_tool)* [**breaking**] Rewrite configuration system with unified agentic-config
- *(config)* [**breaking**] Rewrite configuration system from JSON to TOML

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints

### 🐛 Bug Fixes
- *(thoughts_tool)* Eliminate TOCTOU races in repos.json migration and load
- Add Unix-only compile guards to all Unix-dependent crates
- *(agentic-config)* Address PR #124 v6 review comments
## [0.9.0] - 2026-03-26

### ⛰️  Features
- *(thoughts_tool)* Add pinned reference refs and remote ref discovery

### 🐛 Bug Fixes
- Fix refs to return all
- *(thoughts_tool)* Unify pinned reference handling across mcp and cli
- *(thoughts_tool)* Bound repo ref waits and split pinned ref validation
- *(thoughts_tool)* Normalize pinned ref identity and reject bare prefixes
- *(thoughts_tool)* Tighten pinned ref validation and idempotent response paths
## [0.8.3] - 2026-03-10

### ⚙️  Miscellaneous
- *(deps)* Upgrade 13 dependencies to latest versions
## [0.8.2] - 2026-03-05

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core
## [0.8.1] - 2026-02-10

### ⛰️  Features
- *(thoughts-tool)* Add canonical RepoIdentity for robust reference sync

### 🐛 Bug Fixes
- *(thoughts_tool)* Add error context to I/O operations for consistent debugging
- *(thoughts_tool)* Stop silently swallowing errors in sync and doctor commands
- *(thoughts_tool)* Address race condition and path traversal in repo mapping

### 🚜 Refactor
- *(thoughts_tool)* Replace fs4 with std file locking
## [0.8.0] - 2026-01-30

### ⚙️  Miscellaneous
- *(deps)* Resolve cargo-deny security audit errors

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
- *(xtask)* Add TODO annotation enforcement to verify

### 🐛 Bug Fixes
- Correct mcp integration metadata for thoughts crates
- *(thoughts_tool)* Harden path stripping against trailing slashes in base
## [0.7.0] - 2026-01-05

### 🐛 Bug Fixes
- *(thoughts_tool)* Store absolute tool paths for Linux mergerfs detection
- *(thoughts_tool)* Align test stub fusermount state with path presence
## [0.6.1] - 2026-01-04

### ⚙️  Miscellaneous
- *(agentic_logging)* Release v0.1.1
## [0.6.0] - 2026-01-04

### ⚙️  Miscellaneous
- Address PR review nits

### ⛰️  Features
- *(thoughts_tool)* Add documents module with logs category
- *(thoughts_tool)* Add self-logging for MCP handlers

### 🐛 Bug Fixes
- *(deps)* Add explicit versions to path dependencies for cargo-deny

### 🚜 Refactor
- *(thoughts_tool)* Rename DocumentType::Logs to Log for consistency
## [0.5.1] - 2026-01-03

### 🚜 Refactor
- *(build)* Migrate from Makefile to Just build system
- Update agent guidance to use MCP Just tools instead of shell commands
## [0.5.0] - 2026-01-02

### ⛰️  Features
- *(thoughts_tool)* Implement ENG-234 thoughts improvements
- *(thoughts_tool)* [**breaking**] Remove --allow-main and lock down main/master branches

### 🐛 Bug Fixes
- *(thoughts_tool)* Address PR #82 review comments

### 🚜 Refactor
- *(thoughts_tool)* Remove redundant integration tests per PR review
## [0.4.3] - 2025-12-27

### ⛰️  Features
- *(thoughts_tool)* Add get_template MCP tool for compile-time embedded templates
- *(coding_agent_tools)* Expand spawn_agent description with usage guidance

### 🐛 Bug Fixes
- *(mcp)* Route tracing to stderr in MCP mode to prevent handshake failures
- Correct typos and spelling in templates and prompts

### 📚 Documentation
- *(thoughts_tool)* Add TODO(1) for doc_type API asymmetry issue
- *(thoughts_tool)* Note prompt updates needed when fixing doc_type asymmetry
## [0.4.2] - 2025-12-16

### ⚙️  Miscellaneous
- *(build)* Standardize Makefile targets for local/CI parity
- *(build)* Add fmt check to check-verbose targets across all Makefiles
## [0.4.1] - 2025-12-15

### 🎨 Styling
- *(thoughts_tool)* Tidy version reporting in platform detector logs

### 🐛 Bug Fixes
- *(thoughts_tool)* Enable HTTPS clones via gix reqwest+rustls transport
- *(thoughts_tool)* Configure bare repo HEAD in git_fetch integration tests
- *(thoughts_tool)* Use reset(Hard) for atomic fast-forward updates
## [0.4.0] - 2025-12-11

### ⛰️  Features
- *(thoughts_tool)* Migrate network ops to gitoxide and shell git for 1Password SSH compatibility

### 🐛 Bug Fixes
- *(thoughts_tool)* Address PR review safety and correctness issues
- *(thoughts_tool)* Standardize git command handling in tests with helper module

### 🚜 Refactor
- *(thoughts_tool)* Deduplicate git helper functions
## [0.3.9] - 2025-12-07

### ⛰️  Features
- *(thoughts_tool)* Add add_reference MCP tool for HTTPS repo references
## [0.3.8] - 2025-11-20

### ⚙️  Miscellaneous
- *(thoughts_tool)* Migrate assert_cmd to cargo_bin_cmd macro

### 🐛 Bug Fixes
- Improve shell script trap patterns across monorepo
## [0.3.7] - 2025-10-31

### ⛰️  Features
- *(thoughts_tool)* Add USERNAME credential type for SSH authentication

### 🐛 Bug Fixes
- *(thoughts_tool)* Add SSH authentication for references sync operations
- *(thoughts_tool)* Address remaining PR #50 CodeRabbit comments
## [0.3.6] - 2025-10-20
## [0.3.5] - 2025-10-20

### ⛰️  Features
- *(thoughts_tool)* Add optional descriptions to reference mounts
## [0.3.4] - 2025-10-16

### ⛰️  Features
- *(thoughts_tool)* Implement text formatting for MCP tools

## [0.3.3] - 2025-10-07

### ⚙️  Miscellaneous
- Updated the following local packages: universal-tool-core
