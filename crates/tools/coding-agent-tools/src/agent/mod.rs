//! Agent spawning module for Claude Code subagents.
//!
//! This module provides configuration and utilities for spawning opinionated
//! Claude Code subagents with specific behaviors based on type and location.

pub mod config;
pub mod prompts;

pub use config::build_mcp_config;
pub use config::compose_prompt;
pub use config::enabled_tools_for;
pub use config::model_for;
pub use prompts::ANALYZER_BASE_PROMPT;
pub use prompts::LOCATOR_BASE_PROMPT;
