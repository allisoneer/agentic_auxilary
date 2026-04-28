# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.1.4] - 2026-04-28

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.3] - 2026-04-17

### ⛰️  Features
- *(agentic-bin)* Add binstall release metadata
## [0.1.2] - 2026-04-15

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.1] - 2026-03-27

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.0] - 2026-03-26

### ⚙️  Miscellaneous
- *(config)* Remove JSON-era code remnants from config system
- *(build)* Enable nightly rustfmt and standardize workspace lints
- Resolve PR #127 review feedback and enable taplo verification

### ⛰️  Features
- *(thoughts_tool)* [**breaking**] Rewrite configuration system with unified agentic-config
- *(agentic-config)* [**breaking**] Replace ModelsConfig with tool-specific subagents and reasoning sections
- *(config)* [**breaking**] Rewrite configuration system from JSON to TOML

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints

### 🐛 Bug Fixes
- *(agentic-config)* Address PR #124 v6 review comments
- *(config)* Address follow-up issues from TOML rewrite

### 📚 Documentation
- Write comprehensive CLAUDE.md for agentic-config and agentic-bin
- *(config)* Add PR #127 groups 5, 7, 10, 11 TODO comments

### 🚜 Refactor
- *(agentic-config)* Make loader read-only with in-memory legacy fallback
- *(agentic-config)* Remove premature models deprecation handling
