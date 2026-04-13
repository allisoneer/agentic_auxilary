# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
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
