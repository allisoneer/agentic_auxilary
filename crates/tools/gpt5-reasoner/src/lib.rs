pub mod client;
pub mod errors;
pub mod optimizer;
pub mod template;
pub mod token;
pub mod tools;

mod types;
pub use types::*;

pub mod engine;
pub use engine::gpt5_reasoner_impl;
pub use tools::build_registry;

pub(crate) const PLAN_STRUCTURE_FILENAME: &str = "plan_structure.md";

// NEW: logging utilities
mod logging; // not public; used internally via crate::logging

#[cfg(test)]
pub mod test_support;

// Removed universal-tool-core macros; Tool impls live in tools.rs
