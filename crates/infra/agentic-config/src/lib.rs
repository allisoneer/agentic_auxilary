//! Unified configuration system for the agentic tools ecosystem.
//!
//! This crate provides:
//! - [`AgenticConfig`]: The root configuration type with namespaced sub-configs
//! - [`load_merged`]: Two-layer config loading (global + local) with env overrides
//! - [`schema`]: JSON Schema generation for IDE autocomplete
//! - [`validation`]: Advisory validation that produces warnings
//!
//! # Configuration Precedence (lowest to highest)
//! 1. Default values
//! 2. Global config (`~/.config/agentic/agentic.json`)
//! 3. Local config (`./agentic.json`)
//! 4. Environment variables
//!
//! # Example
//! ```no_run
//! use agentic_config::{load_merged, AgenticConfig};
//! use std::path::Path;
//!
//! let loaded = load_merged(Path::new(".")).unwrap();
//! println!("Default model: {}", loaded.config.models.default_model);
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
//! - `AGENTIC_MODEL_DEFAULT`: Override default model
//! - `AGENTIC_MODEL_REASONING`: Override reasoning model
//! - `AGENTIC_MODEL_FAST`: Override fast model
//! - `AGENTIC_LOG_LEVEL`: Override log level
//! - `AGENTIC_LOG_JSON`: Enable JSON logging ("true" or "1")

pub mod loader;
pub mod merge;
pub mod migration;
pub mod schema;
pub mod types;
pub mod validation;
pub mod writer;

// Re-exports for convenient access
pub use loader::{LoadedAgenticConfig, load_merged};
pub use schema::schema_json_pretty;
pub use types::AgenticConfig;
