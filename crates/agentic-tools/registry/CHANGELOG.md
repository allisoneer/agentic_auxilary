# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.2.2] - 2026-02-10

### ⚙️  Miscellaneous
- Updated the following local packages: coding_agent_tools, gpt5_reasoner, pr_comments, thoughts-mcp-tools
## [0.2.1] - 2026-02-04

### ⛰️  Features
- Add web_fetch and web_search MCP tools with exa-async client

### 🚜 Refactor
- *(web-retrieval)* Rename crate from web-tools to web-retrieval
## [0.2.0] - 2026-02-02

### ⛰️  Features
- *(linear)* [**breaking**] Enrich issue responses with structured metadata and add archive/metadata tools
## [0.1.0] - 2026-01-30

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
- [**breaking**] Rename tools with category-based prefixes and consolidate MCP server
- *(pr_comments)* Add fail-fast errors, JSONL logging, timeouts, and pagination UX

### 🐛 Bug Fixes
- *(registry)* Rename misleading test that contradicts its assertion

### 📚 Documentation
- Documentation update

### 🚜 Refactor
- *(agentic-tools-registry)* Remove unreachable is_empty check
