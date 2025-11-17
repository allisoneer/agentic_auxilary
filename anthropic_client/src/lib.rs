#![deny(warnings)]
#![deny(clippy::all)]

pub mod client;
pub mod config;
pub mod error;
pub mod resources;
pub mod retry;
pub mod sse; // placeholder, feature-gated impl to come
pub mod types;

pub use crate::client::Client;
pub use crate::config::{AnthropicAuth, AnthropicConfig, BetaFeature};
pub use crate::error::{AnthropicError, ApiErrorObject};
