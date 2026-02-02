#![deny(warnings)]
#![deny(clippy::all)]
#![deny(missing_docs)]

//! Async Exa API client with typed requests/responses, retries, and wiremock tests.

/// HTTP client implementation
pub mod client;
/// Configuration types for the client
pub mod config;
/// Error types
pub mod error;
/// API resource implementations
pub mod resources;
/// Retry logic utilities
pub mod retry;
/// Test support utilities (for use in tests)
#[doc(hidden)]
pub mod test_support;
/// Request and response types
pub mod types;

pub use crate::client::Client;
pub use crate::config::ExaConfig;
pub use crate::error::{ApiErrorObject, ExaError};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::types::*;
    pub use crate::{Client, ExaConfig};
}
