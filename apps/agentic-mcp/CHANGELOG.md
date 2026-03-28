# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]
## [0.2.2] - 2026-03-27

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-registry
## [0.2.1] - 2026-03-27

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.2.0] - 2026-03-26

### ⚙️  Miscellaneous
- *(config)* Remove JSON-era code remnants from config system
- *(build)* Enable nightly rustfmt and standardize workspace lints
- Resolve PR #127 review feedback and enable taplo verification

### ⛰️  Features
- *(config)* [**breaking**] Rewrite configuration system from JSON to TOML

### 🎨 Styling
- Apply rustfmt 2024 edition and fix clippy lints

### 🐛 Bug Fixes
- *(config)* Address follow-up issues from TOML rewrite
- Address PR review follow-ups
## [0.1.11] - 2026-03-26

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-registry
## [0.1.10] - 2026-03-26

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-registry
## [0.1.9] - 2026-03-26

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-mcp, agentic-tools-registry
## [0.1.8] - 2026-03-21

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.7] - 2026-03-10

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.6] - 2026-03-05

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.5] - 2026-02-27

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-registry
## [0.1.4] - 2026-02-26

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-registry
## [0.1.3] - 2026-02-10

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.2] - 2026-02-04

### ⚙️  Miscellaneous
- Update Cargo.lock dependencies
## [0.1.1] - 2026-02-02

### ⚙️  Miscellaneous
- Updated the following local packages: agentic-tools-registry
## [0.1.0] - 2026-01-30

### ⛰️  Features
- Agentic-tools framework and monorepo restructure
- [**breaking**] Rename tools with category-based prefixes and consolidate MCP server
- *(agentic-tools)* [**breaking**] Simplify MCP output modes and fix protocol compliance
- *(xtask)* Add README auto-generation with tiered crate listings

### 🐛 Bug Fixes
- *(agentic-mcp)* Resolve rustls CryptoProvider panic on HTTPS requests
