//! Authentication error types

/// Authentication-related errors
#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    /// Network error during authentication
    #[error("Network error: {0}")]
    Network(String),
    /// Storage error when accessing credentials
    #[error("Storage error: {0}")]
    Storage(String),
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
    /// Token error (invalid, expired, etc.)
    #[error("Token error: {0}")]
    Token(String),
    /// Other authentication errors
    #[error("Other: {0}")]
    Other(String),
}
