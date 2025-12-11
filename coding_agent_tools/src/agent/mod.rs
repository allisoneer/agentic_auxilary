//! Agent spawning module for Claude Code subagents.
//!
//! This module provides configuration and utilities for spawning opinionated
//! Claude Code subagents with specific behaviors based on type and location.

pub mod config;
pub mod prompts;

pub use config::{
    allowed_tools_for, build_mcp_config, compose_prompt, model_for, require_binaries_for_location,
    resolve_working_dir,
};
pub use prompts::{ANALYZER_BASE_PROMPT, LOCATOR_BASE_PROMPT};
