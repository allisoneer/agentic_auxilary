# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.3.6] - 2026-04-07

### ⚙️  Miscellaneous
- Updated the following local packages: web-retrieval
## [0.3.5] - 2026-04-06

### ⚙️  Miscellaneous
- Updated the following local packages: gpt5_reasoner, coding_agent_tools, pr_comments, thoughts-mcp-tools
## [0.3.4] - 2026-04-06

### ⚙️  Miscellaneous
- Updated the following local packages: web-retrieval
## [0.3.3] - 2026-03-28

### 🐛 Bug Fixes
- *(linear)* Expose newly added tools in the unified registry
## [0.3.2] - 2026-03-27

### 🐛 Bug Fixes
- *(review_tools)* Align review tool names with review namespace
## [0.3.1] - 2026-03-27

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-config, coding_agent_tools, gpt5_reasoner, review_tools, web-retrieval
## [0.3.0] - 2026-03-26
## [0.2.10] - 2026-03-26

### ⚙️  Miscellaneous
- Updated the following local packages: pr_comments, coding_agent_tools, linear-tools, review_tools
## [0.2.9] - 2026-03-26
## [0.2.8] - 2026-03-26

### 🚜 Refactor
- *(review)* Migrate from standalone MCP server to integrated tool library
## [0.2.7] - 2026-03-21

### ⚙️  Miscellaneous
- Updated the following local packages: coding_agent_tools
## [0.2.6] - 2026-03-10

### ⚙️  Miscellaneous
- Updated the following local packages: coding_agent_tools, gpt5_reasoner, linear-tools, pr_comments, web-retrieval, thoughts-mcp-tools
## [0.2.5] - 2026-03-05

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core, coding_agent_tools, gpt5_reasoner, linear-tools, pr_comments, thoughts-mcp-tools, web-retrieval
## [0.2.4] - 2026-02-27

### ⚙️  Miscellaneous
- Updated the following local packages: web-retrieval
## [0.2.3] - 2026-02-26

### ⚙️  Miscellaneous
- Updated the following local packages: coding_agent_tools, linear-tools, web-retrieval, pr_comments
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
