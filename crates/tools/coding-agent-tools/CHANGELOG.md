# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.2.2] - 2026-02-10

### ⚙️  Miscellaneous
- Updated the following local packages: thoughts-tool
## [0.2.1] - 2026-02-04

### ⛰️  Features
- Add web_fetch and web_search MCP tools with exa-async client
## [0.2.0] - 2026-01-30

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
- [**breaking**] Rename tools with category-based prefixes and consolidate MCP server

### 🐛 Bug Fixes
- *(coding_agent_tools)* Switch subagent MCP config to --allow allowlist
- Align user-facing tool names with unified registry after restructure

### 🚜 Refactor
- Rename spawn_agent internals to ask_agent
## [0.1.8] - 2026-01-05

### ⚙️  Miscellaneous
- Updated the following local packages: thoughts-tool
## [0.1.7] - 2026-01-04

### ⚙️  Miscellaneous
- *(agentic_logging)* Release v0.1.1
## [0.1.6] - 2026-01-04

### ⛰️  Features
- *(coding_agent_tools)* Add JSONL logging for all MCP tools

### 🐛 Bug Fixes
- *(deps)* Add explicit versions to path dependencies for cargo-deny
- *(coding_agent_tools)* Ensure consistent timestamps between markdown and JSONL logs
## [0.1.5] - 2026-01-03

### ⛰️  Features
- *(coding_agent_tools)* Add just recipe search and execute MCP tools

### 🐛 Bug Fixes
- *(coding_agent_tools)* Default just_execute to root justfile when no dir specified
- *(coding_agent_tools)* Canonicalize repo_root in execute_recipe for macOS
- *(coding_agent_tools)* Address PR review comments

### 🚜 Refactor
- *(build)* Migrate from Makefile to Just build system
- *(coding_agent_tools)* Rename search/execute to just_search/just_execute
- *(coding_agent_tools)* Extract test helper macro and fix tool name comment
## [0.1.4] - 2026-01-02

### ⛰️  Features
- *(thoughts_tool)* Implement ENG-234 thoughts improvements
## [0.1.3] - 2025-12-27

### ⛰️  Features
- *(coding_agent_tools)* Add tilde (~) expansion for path arguments
## [0.1.2] - 2025-12-27

### ⛰️  Features
- *(coding_agent_tools)* Add spawn_agent MCP tool for Claude subagents
- *(coding_agent_tools)* Add search_grep and search_glob MCP tools
- *(coding_agent_tools)* Expand sub-agent prompts with verbose strategies and templates
- *(coding_agent_tools)* Add MCP server tool whitelisting via CLI flags
- *(claudecode_rs)* Add MCP config validation with spawn_agent integration
- *(coding_agent_tools)* Expand spawn_agent description with usage guidance

### 🐛 Bug Fixes
- *(coding_agent_tools)* Add version to claudecode path dependency
- *(spawn_agent)* Reject empty/whitespace-only strings as valid output
- *(spawn_agent)* Use three-layer tool filtering for schema control
- *(mcp)* Route tracing to stderr in MCP mode to prevent handshake failures
- *(coding_agent_tools)* Add ls tool to analyzer thoughts and references agents
- *(coding_agent_tools)* Simplify enum schemas for OpenCode compatibility
- Use plural doc_type values when filtering list_active_documents output
- *(coding_agent_tools)* Use prefixed MCP tool names in agent prompts

### 🚜 Refactor
- *(coding_agent_tools)* Remove working directory resolution and redundant tool listings

### 🧪 Testing
- *(coding_agent_tools)* Update tests to match enabled_tools_for changes
## [0.1.1] - 2025-12-16

### ⚙️  Miscellaneous
- *(build)* Standardize Makefile targets for local/CI parity
- *(build)* Add fmt check to check-verbose targets across all Makefiles
