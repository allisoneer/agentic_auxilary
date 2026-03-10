# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.1.0] - 2026-03-10

### ⚙️  Miscellaneous
- *(opencode-orchestrator-mcp)* Align justfile, add compile guard, sanitize logs
- *(opencode-orchestrator-mcp)* Pin self dev-dep version for cargo-deny

### ⛰️  Features
- *(opencode_orchestrator_mcp)* Add MCP server for orchestrator-style agents
- *(opencode_orchestrator_mcp)* Implement lazy server initialization with recursion guard

### 🐛 Bug Fixes
- *(opencode_orchestrator_mcp)* Propagate command dispatch failures to orchestrator_run
- *(opencode_orchestrator_mcp)* Run integration tests single-threaded
- *(opencode_rs)* Correct CommandRequest field name and type for command endpoint
- *(opencode_orchestrator_mcp)* Eliminate hangs via polling-based idle detection
- *(opencode_orchestrator_mcp)* Only track busy state for our own session
- *(opencode_orchestrator_mcp)* Resolve permission response flow bugs
- *(opencode_orchestrator_mcp)* Wait for permission follow-up activity and add idle timeout
- *(opencode-orchestrator-mcp)* Recompute threshold when context_limit set after tokens
- *(opencode-orchestrator-mcp)* Support permission_request_id in respond_permission

### 📚 Documentation
- *(opencode_orchestrator_mcp)* Add README with quick commands and local validation guide

### 🚜 Refactor
- *(opencode_orchestrator_mcp)* [**breaking**] Rename MCP tools to short names and add xtask indexing

### 🧪 Testing
- *(opencode_orchestrator_mcp)* Add env-gated integration coverage and local recipes
