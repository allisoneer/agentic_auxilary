# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.3.1] - 2026-03-26

### ⚙️  Miscellaneous
- Resolve PR #127 review feedback and enable taplo verification

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints
## [0.3.0] - 2026-03-10

### ⚙️  Miscellaneous
- *(opencode_rs)* Strengthen test assertion and remove dead code

### ⛰️  Features
- *(opencode_rs)* Add transport-level HTTP retry for command dispatch
- *(opencode_orchestrator_mcp)* Implement lazy server initialization with recursion guard

### 🐛 Bug Fixes
- *(opencode_rs)* Resolve clippy pedantic warnings
- *(opencode_rs)* Handle 204 No Content response from prompt_async endpoint
- *(opencode_rs)* Correct CommandRequest field name and type for command endpoint
- *(opencode_rs)* Handle empty tool objects in permission request deserialization
- *(opencode_rs)* Use correct uppercase ID field names for OpenCode compatibility
- *(opencode_rs)* Match permission reply return type to actual API response
- *(opencode_rs)* [**breaking**] Align SDK response types with OpenAPI spec
- *(opencode_rs)* Align session status parsing and extend default timeout
- *(opencode_rs)* Add message_id to CommandRequest for idempotent retries
## [0.2.0] - 2026-02-27

### ⛰️  Features
- *(opencode_rs)* [**breaking**] Align Session, MessageInfo, and Model schemas with upstream
- *(opencode_rs)* [**breaking**] Complete upstream parity (phases 4-9)

### 🐛 Bug Fixes
- *(opencode_rs)* Resolve runtime issues found when testing against real server

### 🚜 Refactor
- *(opencode_rs)* [**breaking**] Resolve all clippy warnings
## [0.1.2] - 2026-01-30

### ⚙️  Miscellaneous
- *(deps)* Resolve cargo-deny security audit errors

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
## [0.1.1] - 2026-01-04
