//! Server-Sent Events (SSE) streaming support.
//!
//! This module provides streaming response handling for the Messages API.
//! Currently a placeholder - full implementation coming in a future release.

#[cfg(feature = "streaming")]
/// Streaming API implementation (placeholder)
///
/// This module will contain the full SSE streaming implementation in a future release.
pub mod streaming {
    use futures::Stream;
    use std::pin::Pin;

    /// Placeholder for SSE event types
    #[derive(Debug, Clone)]
    pub enum Event {
        /// Message start event
        MessageStart,
        /// Content block start event
        ContentBlockStart,
        /// Content block delta with text
        ContentBlockDelta {
            /// The text delta
            text: String,
        },
        /// Message delta event
        MessageDelta,
        /// Message stop event
        MessageStop,
        /// Error event
        Error {
            /// Error message
            message: String,
        },
    }

    /// Placeholder for streaming response
    pub type EventStream =
        Pin<Box<dyn Stream<Item = Result<Event, crate::error::AnthropicError>> + Send>>;

    // Future: implement SSE parsing and stream mapping
}
