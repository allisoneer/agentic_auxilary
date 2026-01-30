//! Rust SDK for OpenCode (HTTP-first hybrid with SSE streaming).
//!
//! This crate provides a native Rust interface to OpenCode's HTTP REST API
//! and SSE streaming capabilities.

#![deny(rust_2018_idioms)]

// Unix-only support
#[cfg(not(unix))]
compile_error!(
    "opencode_rs only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

pub mod error;
pub mod types;

#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "sse")]
pub mod sse;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "cli")]
pub mod cli;

// Public ergonomic API
pub mod client;

// Re-exports
pub use crate::client::{Client, ClientBuilder};
pub use crate::error::{OpencodeError, Result};

// Version info
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
