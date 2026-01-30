//! Agent spawning module for Claude Code subagents.
//!
//! This module provides configuration and utilities for spawning opinionated
//! Claude Code subagents with specific behaviors based on type and location.

pub mod config;
pub mod prompts;

pub use config::{build_mcp_config, compose_prompt, enabled_tools_for, model_for};
pub use prompts::{ANALYZER_BASE_PROMPT, LOCATOR_BASE_PROMPT};
