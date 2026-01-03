//! Error types for opencode_rs.

use thiserror::Error;

/// Result type alias for opencode_rs operations.
pub type Result<T> = std::result::Result<T, OpencodeError>;

/// Error type for opencode_rs operations.
#[derive(Debug, Error)]
pub enum OpencodeError {
    /// HTTP request error.
    #[cfg(feature = "http")]
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// SSE streaming error.
    #[error("SSE error: {0}")]
    Sse(String),

    /// JSON serialization/deserialization error.
    #[cfg(feature = "http")]
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// URL parsing error.
    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Unexpected HTTP status code.
    #[error("Unexpected status {status}: {body}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
        /// Response body.
        body: String,
    },

    /// Invalid configuration.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Stream closed unexpectedly.
    #[error("Stream closed unexpectedly")]
    StreamClosed,

    /// Session not found.
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// Internal state error.
    #[error("Internal state error: {0}")]
    State(String),
}
