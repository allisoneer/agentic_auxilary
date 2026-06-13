//! Programmatic gwt-compatible git worktree management.

pub mod command;
pub mod config;
pub mod error;
pub mod exec;
pub mod plan;
pub mod pr;
pub mod remote;
pub mod repo;
pub mod types;
pub mod worktree;

pub use error::Error;
pub use error::Result;
