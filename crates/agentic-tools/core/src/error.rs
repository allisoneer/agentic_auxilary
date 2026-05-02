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
    pub fn invalid_input<S: ToString>(s: S) -> Self {
        ToolError::InvalidInput(s.to_string())
    }

    /// Create an internal error.
    pub fn internal<S: ToString>(s: S) -> Self {
        ToolError::Internal(s.to_string())
    }

    /// Create an external service error.
    pub fn external<S: ToString>(s: S) -> Self {
        ToolError::External(s.to_string())
    }

    /// Create a not found error.
    pub fn not_found<S: ToString>(s: S) -> Self {
        ToolError::NotFound(s.to_string())
    }

    /// Create a permission denied error.
    pub fn permission<S: ToString>(s: S) -> Self {
        ToolError::Permission(s.to_string())
    }

    /// Create a cancelled error.
    pub fn cancelled(reason: Option<String>) -> Self {
        ToolError::Cancelled { reason }
    }
}
