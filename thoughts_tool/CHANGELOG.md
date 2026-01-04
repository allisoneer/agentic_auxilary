# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
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
