# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.6.2] - 2026-02-26

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-utils
## [0.6.1] - 2026-02-10

### ⚙️  Miscellaneous
- Updated the following local packages: thoughts-tool
## [0.6.0] - 2026-01-30

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
- [**breaking**] Rename tools with category-based prefixes and consolidate MCP server
- *(pr_comments)* Add fail-fast errors, JSONL logging, timeouts, and pagination UX
## [0.5.2] - 2026-01-03

### ⛰️  Features
- *(pr_comments)* Default show_ids to true for MCP text output

### 🚜 Refactor
- *(build)* Migrate from Makefile to Just build system
## [0.5.1] - 2026-01-02

### ⚙️  Miscellaneous
- Updated the following local packages: universal-tool-core
## [0.5.0] - 2026-01-02

### ⛰️  Features
- *(pr_comments)* [**breaking**] Simplify API to 3 tools with thread pagination and reply support
## [0.4.3] - 2025-12-27

### 🐛 Bug Fixes
- *(mcp)* Route tracing to stderr in MCP mode to prevent handshake failures
## [0.4.2] - 2025-12-16

### ⚙️  Miscellaneous
- *(build)* Standardize Makefile targets for local/CI parity
- *(build)* Add fmt check to check-verbose targets across all Makefiles
## [0.4.1] - 2025-11-20

### 🐛 Bug Fixes
- Improve shell script trap patterns across monorepo
## [0.4.0] - 2025-11-04

### ⛰️  Features
- *(pr_comments)* Add thread support, author filtering, and pagination

### 🐛 Bug Fixes
- *(pr_comments)* Prevent orphaned replies in get_review_comments filter pipeline
## [0.3.0] - 2025-10-20

### ⚙️  Miscellaneous
- *(metadata)* Add missing keywords and categories to pr_comments and gpt5_reasoner

### ⛰️  Features
- *(pr_comments)* Add MCP text formatting with 50-65% token reduction
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
