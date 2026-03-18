//! Unified configuration system for the agentic tools ecosystem.
//!
//! This crate provides:
//! - [`AgenticConfig`]: The root configuration type with namespaced sub-configs
//! - [`load_merged`]: Two-layer config loading (global + local) with env overrides
//! - [`schema`]: JSON Schema generation for IDE autocomplete (Taplo support)
//! - [`validation`]: Advisory validation that produces warnings
//!
//! # Configuration Precedence (lowest to highest)
//! 1. Default values
//! 2. Global config (`~/.config/agentic/agentic.toml`)
//! 3. Local config (`./agentic.toml`)
//! 4. Environment variables
//!
//! # Example
//! ```no_run
//! use agentic_config::{load_merged, AgenticConfig};
//! use std::path::Path;
//!
//! let loaded = load_merged(Path::new(".")).unwrap();
//! println!("Locator model: {}", loaded.config.subagents.locator_model);
//!
//! for warning in &loaded.warnings {
//!     eprintln!("Warning: {}", warning);
//! }
//! ```
//!
//! # Environment Variables
//! - `ANTHROPIC_BASE_URL`: Override Anthropic API base URL
//! - `ANTHROPIC_API_KEY`: Set Anthropic API key (env-only)
//! - `EXA_BASE_URL`: Override Exa API base URL
//! - `EXA_API_KEY`: Set Exa API key (env-only)
//! - `AGENTIC_SUBAGENTS_LOCATOR_MODEL`: Override `subagents.locator_model`
//! - `AGENTIC_SUBAGENTS_ANALYZER_MODEL`: Override `subagents.analyzer_model`
//! - `AGENTIC_REASONING_OPTIMIZER_MODEL`: Override `reasoning.optimizer_model`
//! - `AGENTIC_REASONING_EXECUTOR_MODEL`: Override `reasoning.executor_model`
//! - `AGENTIC_REASONING_EFFORT`: Override `reasoning.reasoning_effort`
//! - `AGENTIC_LOG_LEVEL`: Override log level
//! - `AGENTIC_LOG_JSON`: Enable JSON logging ("true" or "1")

#[cfg(not(unix))]
compile_error!(
    "agentic-config only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

pub mod loader;
pub mod merge;
pub mod paths;
pub mod schema;
#[cfg(test)]
pub(crate) mod test_support;
pub mod types;
pub mod validation;

// Re-exports for convenient access
pub use loader::{LoadedAgenticConfig, load_merged};
pub use paths::{agentic_config_dir, xdg_config_home};
pub use schema::schema_json_pretty;
pub use types::AgenticConfig;
