# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.8.0] - 2026-04-28

### ⚙️  Miscellaneous
- *(opencode)* Pin OpenCode to v1.14.19

### ⛰️  Features
- *(opencode_rs)* [**breaking**] Align SDK contracts with OpenCode v1.14.19

### 🐛 Bug Fixes
- *(opencode_rs)* Align bounded PR 182 review fixes with actual contracts
- Address bounded PR 182 review fixes

### 🚜 Refactor
- *(opencode_rs)* Remove snapshot APIs and simplify skills surface
## [0.7.0] - 2026-04-13

### ⛰️  Features
- *(opencode_rs)* Add batch session status lookup

### 🐛 Bug Fixes
- *(orchestrator)* Resolve bounded PR168 review threads
## [0.6.0] - 2026-04-07

### ⛰️  Features
- *(opencode)* Upgrade SDK compatibility to opencode v1.3.17
- *(opencode_rs)* Add missing SSE event types from OpenCode 1.3.17

### 🐛 Bug Fixes
- *(opencode_rs)* Align type definitions with OpenCode 1.3.17 schema
- *(opencode_rs)* Correct field casing to match OpenCode 1.3.17 convention
- *(opencode_rs)* Ensure typed tests initialize server directly under feature="server"
## [0.5.0] - 2026-04-06

### ⛰️  Features
- *(opencode_rs)* Add version pinning and E2E test infrastructure
- *(opencode_rs)* Add DELETE message and POST git/init endpoints
- *(opencode_rs)* Add endpoint coverage verification tooling

### 🧪 Testing
- *(opencode_rs)* Add wiremock unit tests for all HTTP modules
## [0.4.0] - 2026-03-27

### ⛰️  Features
- *(opencode-orchestrator-mcp)* Add stable v1.3.3 launcher support and reliability fixes
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
