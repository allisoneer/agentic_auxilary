# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.1.8] - 2026-03-27

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.7] - 2026-03-26

### ⚙️  Miscellaneous
- *(config)* Remove JSON-era code remnants from config system
- Resolve PR #127 review feedback and enable taplo verification

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints
## [0.1.6] - 2026-03-26

### 🐛 Bug Fixes
- *(thoughts_tool)* Unify pinned reference handling across mcp and cli
## [0.1.5] - 2026-03-21

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.4] - 2026-03-10

### ⚙️  Miscellaneous
- *(deps)* Upgrade 13 dependencies to latest versions
## [0.1.3] - 2026-03-05

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.2] - 2026-02-10

### ⛰️  Features
- *(thoughts-tool)* Add canonical RepoIdentity for robust reference sync
- *(thoughts)* Make doctor --fix authoritative and add canonical matching to remove

### 🐛 Bug Fixes
- *(thoughts_tool)* Stop silently swallowing errors in sync and doctor commands
- *(thoughts_tool)* Address race condition and path traversal in repo mapping

### 🚜 Refactor
- *(thoughts)* Remove unused _mapping_path parameter from apply_fixes
## [0.1.1] - 2026-02-04

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
