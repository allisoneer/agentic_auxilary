# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.7.0] - 2026-06-05

### ⛰️  Features
- *(opencode-orchestrator-mcp)* Add explicit agent listing and selection
## [0.6.7] - 2026-06-03

### ⚙️  Miscellaneous
- Updated the following local packages: thoughts-tool
## [0.6.6] - 2026-06-02

### 🐛 Bug Fixes
- *(opencode-orchestrator-mcp)* Harden permission reply bridge
- *(opencode-orchestrator-mcp)* Harden permission reply fallback
## [0.6.5] - 2026-05-22

### ⚙️  Miscellaneous
- *(opencode)* Pin opencode 1.15.7
## [0.6.4] - 2026-05-22

### ⚙️  Miscellaneous
- Updated the following local packages: opencode_rs
## [0.6.3] - 2026-05-20

### ⛰️  Features
- Add configurable runtime timeouts across agentic tools

### 🐛 Bug Fixes
- *(logging)* Address PR #220 timeout and failure handling review fixes
## [0.6.2] - 2026-05-19

### ⛰️  Features
- *(opencode-orchestrator-mcp)* Add command allow and deny filtering
## [0.6.1] - 2026-05-07

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core, agentic_logging, agentic-tools-mcp, thoughts-tool
## [0.6.0] - 2026-05-06

### 🐛 Bug Fixes
- *(opencode-orchestrator-mcp)* Avoid lost release notification in test helper
- *(opencode-orchestrator-mcp)* Recover managed handles after server loss
## [0.5.1] - 2026-05-04

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-config
## [0.5.0] - 2026-05-04

### ⛰️  Features
- *(opencode-orchestrator-mcp)* Expose session paths in session tools

### 📚 Documentation
- *(opencode-orchestrator-mcp)* Correct tool count to six and add get_session_state
## [0.4.10] - 2026-05-03

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-config
## [0.4.9] - 2026-05-03

### ⛰️  Features
- *(tools)* Apply cancellation to long-running tools
## [0.4.8] - 2026-05-01
## [0.4.7] - 2026-04-29

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.4.6] - 2026-04-28

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.4.5] - 2026-04-17

### ⚙️  Miscellaneous
- Updated the following local packages: thoughts-tool
## [0.4.4] - 2026-04-16

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.4.3] - 2026-04-15

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core, agentic-tools-mcp, thoughts-tool
## [0.4.2] - 2026-04-13

### ⛰️  Features
- Add cargo-binstall metadata to app crates
## [0.4.1] - 2026-04-13

### ⚙️  Miscellaneous
- Updated the following local packages: thoughts-tool
## [0.4.0] - 2026-04-13

### ⛰️  Features
- *(opencode_orchestrator_mcp)* Add session troubleshooting diagnostics

### 🐛 Bug Fixes
- *(opencode_orchestrator_mcp)* Address bounded PR 168 review findings
- *(orchestrator)* Resolve bounded PR168 review threads
- *(orchestrator)* Treat missing status-map entries as idle
- *(opencode_orchestrator_mcp)* Fall back to session metadata for last activity
## [0.3.4] - 2026-04-07

### ⛰️  Features
- *(opencode)* Upgrade SDK compatibility to opencode v1.3.17
- *(opencode_orchestrator_mcp)* Add config injection for integration tests

### 🧪 Testing
- *(opencode_orchestrator_mcp)* Expand wiremock test coverage
- *(opencode_orchestrator_mcp)* Verify ID-specific routing in respond_question_by_id_lookup
## [0.3.3] - 2026-04-06
## [0.3.2] - 2026-04-06
## [0.3.1] - 2026-03-27
## [0.3.0] - 2026-03-27

### ⛰️  Features
- *(opencode-orchestrator-mcp)* Add stable v1.3.3 launcher support and reliability fixes

### 🐛 Bug Fixes
- *(opencode-orchestrator-mcp)* Address bounded PR comment batch

### 🧪 Testing
- *(opencode-orchestrator-mcp)* Migrate env tests from ENV_LOCK to serial_test
## [0.2.0] - 2026-03-26

### ⚙️  Miscellaneous
- Resolve PR #127 review feedback and enable taplo verification

### ⛰️  Features
- *(config)* [**breaking**] Rewrite configuration system from JSON to TOML

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints
## [0.1.2] - 2026-03-26

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-mcp
## [0.1.1] - 2026-03-21

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
