//! `OpenCode` Orchestrator MCP - library exports for integration testing.

#[cfg(not(unix))]
compile_error!(
    "opencode_orchestrator_mcp only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

pub mod config;
pub mod logging;
pub mod server;
pub mod token_tracker;
pub mod tools;
pub mod types;
pub mod version;
