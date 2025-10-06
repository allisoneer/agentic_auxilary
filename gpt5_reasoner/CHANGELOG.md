# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

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
