# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.2.1] - 2025-10-07

### ⚙️  Miscellaneous
- Updated the following local packages: universal-tool-core
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

### Added
- Directory support: Accept directories via `DirectoryMeta` with automatic file expansion
- Configurable directory traversal: recursive, hidden files, extension filtering, max_files cap
- Binary file detection and automatic skipping during directory expansion
- Path normalization to absolute paths (without symlink resolution)
- Example JSON files for directory usage (`examples/directories.json`, `examples/empty_files.json`)
- Comprehensive directory expansion tests (7 new tests)
- Model selection tests with proper thread-safe env var handling (3 new tests)

### Changed
- **BREAKING**: Default optimizer model changed from `openai/gpt-5` to `anthropic/claude-sonnet-4.5`
- **BREAKING**: `gpt5_reasoner_impl` signature now includes `directories: Option<Vec<DirectoryMeta>>` parameter
- **BREAKING**: `optimize_and_execute` MCP/CLI function now accepts optional `directories` parameter
- CLI now accepts `--directories-json` for directory-based file discovery
- Total test count increased from 30 to 40 tests

### Notes
- Executor model remains unchanged: `openai/gpt-5`
- OPTIMIZER_MODEL precedence: parameter > env var > default
- Directory expansion happens before optimizer sees files
- Deduplication ensures files don't appear twice if listed in both files and directories
- Hidden directories are pruned from traversal when `include_hidden=false`
- Extension filter is case-insensitive and accepts both "rs" and ".rs" formats

## [0.1.0] - 2025-10-06

### Added
- Initial release of gpt5_reasoner tool
- Two-phase prompt optimization (optimizer → executor pattern)
- Support for reasoning and plan prompt types
- Defensive plan template injection with executor-side guards
- Application-level retry logic for network errors
- Token counting and enforcement (250k limit)
- Dual CLI and MCP interfaces via universal-tool framework
- Concurrent file reading for performance
- Comprehensive test coverage (30 tests)
