# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.1.2] - 2026-01-30

### ⛰️  Features
- Agentic-tools framework and monorepo restructure

### 🚜 Refactor
- Rename spawn_agent internals to ask_agent
## [0.1.1] - 2026-01-04

### ⚙️  Miscellaneous
- Release
## [0.1.0] - 2026-01-04

### ⚙️  Miscellaneous
- Address additional PR review nits

### ⛰️  Features
- Add agentic_logging crate for centralized JSONL logging

### 🐛 Bug Fixes
- *(gpt5_reasoner)* Complete JSONL logging for all execution paths
- *(coding_agent_tools)* Ensure consistent timestamps between markdown and JSONL logs
- *(agentic_logging)* Make logging disabled check case-insensitive
