# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.6.4] - 2026-07-01

### 🐛 Bug Fixes
- *(xtask)* Generate crate claude commands with just wrappers
## [0.6.3] - 2026-06-29

### 🐛 Bug Fixes
- *(linear)* Clarify full ticket read guidance
## [0.6.2] - 2026-06-18

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-config
## [0.6.1] - 2026-06-05

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-config
## [0.6.0] - 2026-05-20

### ⛰️  Features
- Add configurable runtime timeouts across agentic tools
## [0.5.4] - 2026-05-07

### 🐛 Bug Fixes
- *(linear)* Satisfy workspace lint checks
- *(linear)* Restore lint-clean linear-tools after main merge
- Address bounded PR review issues
## [0.5.3] - 2026-05-03

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core, agentic-tools-mcp
## [0.5.2] - 2026-04-15

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core, agentic-tools-mcp
## [0.5.1] - 2026-04-13

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core, agentic-tools-mcp
## [0.5.0] - 2026-03-28

### ⛰️  Features
- *(linear-tools)* Add update_issue, set_relation, and creator_id filter
- *(linear-tools)* Add linear_get_issue_comments tool with pagination
- *(linear-tools)* Add URL to issue results and comments formatting

### 🐛 Bug Fixes
- *(linear-tools)* Address PR review feedback for issue operations
- *(linear-tools)* Fetch all issue comment pages before local pagination
## [0.4.6] - 2026-03-26

### ⚙️  Miscellaneous
- Resolve PR #127 review feedback and enable taplo verification

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints
## [0.4.5] - 2026-03-26

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-utils
## [0.4.4] - 2026-03-26

### 🚜 Refactor
- *(policy)* Reserve mcp=true for runtime MCP server apps only
## [0.4.3] - 2026-03-10

### ⚙️  Miscellaneous
- *(deps)* Upgrade 13 dependencies to latest versions
## [0.4.2] - 2026-03-05

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-core, agentic-tools-mcp
## [0.4.1] - 2026-02-26
## [0.4.0] - 2026-02-02

### ⛰️  Features
- *(linear)* [**breaking**] Enrich issue responses with structured metadata and add archive/metadata tools
## [0.3.0] - 2026-01-30

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
- [**breaking**] Rename tools with category-based prefixes and consolidate MCP server

### 🐛 Bug Fixes
- *(agentic-mcp)* Resolve rustls CryptoProvider panic on HTTPS requests
