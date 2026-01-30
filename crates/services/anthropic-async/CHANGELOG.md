# Changelog

## [Unreleased]
## [0.2.1] - 2026-01-03

### 🚜 Refactor
- *(build)* Migrate from Makefile to Just build system
## [0.2.0] - 2025-12-16

### ⚙️  Miscellaneous
- *(build)* Standardize Makefile targets for local/CI parity
- *(build)* Add fmt check to check-verbose targets across all Makefiles

### ⛰️  Features
- *(anthropic_async)* Add streaming, structured outputs, and tool enhancements
- *(anthropic_async)* Add forward-compatible Unknown event handling for SSE streams

### 🚜 Refactor
- *(anthropic_async)* Deduplicate validation and remove unused placeholder
## [0.1.0] - 2025-11-20

### ⚙️  Miscellaneous
- *(anthropic_async)* Add Cargo.lock files for examples

### ⛰️  Features
- *(anthropic_async)* Rename anthropic_client to anthropic-async and integrate into workspace
- *(anthropic_async)* [**breaking**] Add dual auth support and improve error handling
- *(anthropic_async)* Add request/response type separation and parameter validation
- *(anthropic_async)* Add type-safe tool calling with schemars
- *(anthropic_async)* Add multimodal content support
- *(anthropic_async)* Add config and pagination improvements
- *(anthropic_async)* Update examples and add tool-calling demonstration

### 🐛 Bug Fixes
- Improve shell script trap patterns across monorepo
- *(anthropic_async)* Resolve clippy warnings and improve code quality
- *(anthropic_async)* Resolve PR review comments for tools module
- Initial rename from anthropic_client to anthropic-async and full workspace integration.
