# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
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
