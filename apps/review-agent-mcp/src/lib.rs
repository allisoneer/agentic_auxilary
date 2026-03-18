//! review-agent-mcp - library exports for testing.

#[cfg(not(unix))]
compile_error!(
    "review_agent_mcp only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

pub mod prompts;
pub mod tools;
pub mod types;
pub mod validation;
