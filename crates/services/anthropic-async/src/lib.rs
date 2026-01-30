#![deny(warnings)]
#![deny(clippy::all)]
#![deny(missing_docs)]

//! # `anthropic-async`
//!
//! A production-ready Anthropic API client for Rust with prompt caching support.
//!
//! ## Quick Start
//!
//! ```no_run
//! use anthropic_async::{Client, types::{content::*, messages::*}};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Client::new();
//!
//! let req = MessagesCreateRequest {
//!     model: "claude-3-5-sonnet".into(),
//!     max_tokens: 100,
//!     messages: vec![MessageParam {
//!         role: MessageRole::User,
//!         content: "Hello!".into(),
//!     }],
//!     system: None,
//!     temperature: None,
//!     stop_sequences: None,
//!     top_p: None,
//!     top_k: None,
//!     metadata: None,
//!     tools: None,
//!     tool_choice: None,
//!     stream: None,
//!     output_format: None,
//! };
//!
//! let response = client.messages().create(req).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Authentication
//!
//! The client supports API key and bearer token authentication.
//! See [`AnthropicConfig`] for configuration options.
//!
//! ## Prompt Caching
//!
//! Use [`CacheControl`](types::common::CacheControl) to cache prompts and reduce costs.

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
/// Server-sent events (streaming) support
#[cfg(feature = "streaming")]
pub mod sse;
/// Hidden SSE module when streaming is not enabled
#[cfg(not(feature = "streaming"))]
pub(crate) mod sse;
/// Test support utilities (for use in tests)
#[doc(hidden)]
pub mod test_support;
/// Request and response types
pub mod types;

pub use crate::client::Client;
pub use crate::config::{AnthropicAuth, AnthropicConfig, BetaFeature};
pub use crate::error::{AnthropicError, ApiErrorObject};

/// Streaming types (requires `streaming` feature)
#[cfg(feature = "streaming")]
pub mod streaming {
    pub use crate::sse::streaming::{
        Accumulator, ContentBlockDeltaData, ContentBlockStartData, Event, EventError, EventStream,
        MessageDeltaPayload, MessageDeltaUsage, MessageStartPayload, MessageStartUsage, SSEDecoder,
        SseFrame, event_stream_from_response,
    };
}

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::types::common::*;
    pub use crate::types::messages::*;
    pub use crate::types::models::*;
    pub use crate::{AnthropicConfig, Client};
}
