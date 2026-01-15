//! Code generation modules for Universal Tool Framework
//!
//! This module contains the code generation logic for all supported interfaces:
//! - CLI (Command Line Interface)
//! - REST (HTTP API)
//! - MCP (Model Context Protocol)
//!
//! The generation is split into shared utilities and interface-specific modules.

pub mod error_handling;
pub mod shared;
pub mod structs;
pub mod types;
pub mod validation;

// Interface-specific modules
pub mod cli; // CLI generation (Task 4 - implemented)
pub mod mcp;
pub mod rest; // REST generation (Task 6 - implemented) // MCP generation (Task 8 - implemented)

// Re-export commonly used types
// pub use shared::*;
// pub use structs::*;
// pub use types::*;
