# Changelog

## [Unreleased]
## [0.5.1] - 2026-03-10

### ⚙️  Miscellaneous
- *(deps)* Upgrade 13 dependencies to latest versions
## [0.5.0] - 2026-02-26

### ⛰️  Features
- *(anthropic-async)* Expand content block types for SDK alignment
- *(anthropic-async)* Add request config types and echo pattern conversions
- *(anthropic-async)* Add httpmock recording infrastructure for conformance tests
- *(anthropic-async)* Complete conformance test infrastructure (phases 5-6)

### 🐛 Bug Fixes
- *(anthropic-async)* Complete TTL validation across all 12 cacheable locations
- *(anthropic-async)* Redact sensitive headers from cassette recordings
- *(anthropic-async)* Resolve clippy lint violations
- *(anthropic-async)* Address PR review comments for test infrastructure

### 🚜 Refactor
- *(anthropic-async)* Eliminate underscore-prefixed variable patterns
- *(anthropic-async)* Migrate to workspace lint inheritance

### 🧪 Testing
- *(anthropic-async)* Add conformance test infrastructure and multi-turn tests
## [0.4.0] - 2026-02-04

### 🐛 Bug Fixes
- *(anthropic-async)* Redact credentials from Debug and reject empty keys

### 🚜 Refactor
- *(services)* Use SecretString for credential handling
## [0.3.0] - 2026-01-30

### ⚙️  Miscellaneous
- *(deps)* Resolve cargo-deny security audit errors

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
- *(xtask)* Add README auto-generation with tiered crate listings

### 🐛 Bug Fixes
- *(anthropic-async)* Use schemars v1 API for schema_for! macro
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
