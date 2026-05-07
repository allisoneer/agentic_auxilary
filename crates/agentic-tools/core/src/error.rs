//! Unified error type for agentic tools.

use thiserror::Error;

/// Error type returned by tool operations.
#[derive(Error, Debug)]
pub enum ToolError {
    /// Invalid input provided to the tool.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Internal error during tool execution.
    #[error("internal error: {0}")]
    Internal(String),

    /// Error from an external service.
    #[error("external service error: {0}")]
    External(String),

    /// Permission denied for the operation.
    #[error("permission denied: {0}")]
    Permission(String),

    /// Requested resource not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Tool execution stopped because the caller cancelled the request.
    #[error(
        "cancelled{}",
        .reason
            .as_deref()
            .map_or_else(String::new, |reason| format!(": {reason}"))
    )]
    Cancelled {
        /// Optional detail about the cancellation reason.
        reason: Option<String>,
    },
}

impl ToolError {
    /// Create an invalid input error.
    pub fn invalid_input(s: impl Into<String>) -> Self {
        Self::InvalidInput(s.into())
    }

    /// Create an internal error.
    pub fn internal(s: impl Into<String>) -> Self {
        Self::Internal(s.into())
    }

    /// Create an external service error.
    pub fn external(s: impl Into<String>) -> Self {
        Self::External(s.into())
    }

    /// Create a not found error.
    pub fn not_found(s: impl Into<String>) -> Self {
        Self::NotFound(s.into())
    }

    /// Create a permission denied error.
    pub fn permission(s: impl Into<String>) -> Self {
        Self::Permission(s.into())
    }

    /// Create a cancelled error.
    pub fn cancelled(reason: Option<String>) -> Self {
        Self::Cancelled { reason }
    }
}
