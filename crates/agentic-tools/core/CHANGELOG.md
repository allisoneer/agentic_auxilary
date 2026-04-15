# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.2.0] - 2026-04-15

### ⛰️  Features
- *(schema)* Strip null from optional properties in MCP input schemas
- *(subagents)* Route search and web tools through agentic-mcp

### 🐛 Bug Fixes
- *(schema)* Use null-first ordering instead of stripping null
- *(agentic_tools_core)* Gate null guidance for optional schema properties

### 📚 Documentation
- *(schema)* Clarify non-string description handling
## [0.1.3] - 2026-04-13

### 🐛 Bug Fixes
- *(schema)* Clarify option docs and stabilize tests

### 🚜 Refactor
- *(agentic-tools-core)* Remove AddNullable transform from schema generation
## [0.1.2] - 2026-03-26

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints
## [0.1.1] - 2026-03-05

### ⚙️  Miscellaneous
- *(deps)* Upgrade rmcp from 0.12.0 to 1.1.0
