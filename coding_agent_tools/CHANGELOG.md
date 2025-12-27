# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
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
