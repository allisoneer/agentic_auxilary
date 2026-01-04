//! Error types for opencode_rs.

use thiserror::Error;

/// Result type alias for opencode_rs operations.
pub type Result<T> = std::result::Result<T, OpencodeError>;

/// Error type for opencode_rs operations.
#[derive(Debug, Error)]
pub enum OpencodeError {
    /// HTTP error with structured response from OpenCode.
    #[error("HTTP error {status}: {message}")]
    Http {
        /// HTTP status code.
        status: u16,
        /// Error name from OpenCode's NamedError (e.g., "NotFound", "ValidationError").
        name: Option<String>,
        /// Error message.
        message: String,
        /// Additional error data.
        data: Option<serde_json::Value>,
    },

    /// Network/connection error.
    #[error("Network error: {0}")]
    Network(String),

    /// SSE streaming error.
    #[error("SSE error: {0}")]
    Sse(String),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// URL parsing error.
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    /// Failed to spawn server process.
    #[error("Failed to spawn server: {message}")]
    SpawnServer {
        /// Error message.
        message: String,
    },

    /// Server not ready within timeout.
    #[error("Server not ready within {timeout_ms}ms")]
    ServerTimeout {
        /// Timeout in milliseconds.
        timeout_ms: u64,
    },

    /// Process execution error.
    #[error("Process error: {0}")]
    Process(String),

    /// Invalid configuration.
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

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

/// Helper to parse OpenCode's NamedError response body.
#[derive(Debug, Clone)]
pub struct HttpErrorBody {
    /// Error name (e.g., "NotFound", "ValidationError").
    pub name: Option<String>,
    /// Error message.
    pub message: Option<String>,
    /// Additional error data.
    pub data: Option<serde_json::Value>,
}

impl HttpErrorBody {
    /// Parse from a JSON value.
    pub fn from_json(v: serde_json::Value) -> Self {
        Self {
            name: v
                .get("name")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            message: v
                .get("message")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            data: v.get("data").cloned(),
        }
    }
}

impl OpencodeError {
    /// Create an HTTP error from status and optional JSON body.
    pub fn http(status: u16, body_text: &str) -> Self {
        // Try to parse as JSON to extract NamedError fields
        let parsed: Option<serde_json::Value> = serde_json::from_str(body_text).ok();
        let info = parsed.clone().map(HttpErrorBody::from_json);

        Self::Http {
            status,
            name: info.as_ref().and_then(|i| i.name.clone()),
            message: info
                .as_ref()
                .and_then(|i| i.message.clone())
                .unwrap_or_else(|| format!("HTTP {}", status)),
            data: info.and_then(|i| i.data),
        }
    }

    /// Check if this is a "not found" error (404).
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Http { status: 404, .. })
    }

    /// Check if this is a validation error (400).
    pub fn is_validation_error(&self) -> bool {
        matches!(self, Self::Http { status: 400, .. })
    }

    /// Check if this is a server error (5xx).
    pub fn is_server_error(&self) -> bool {
        matches!(self, Self::Http { status, .. } if *status >= 500)
    }

    /// Get the error name if this is an HTTP error.
    pub fn error_name(&self) -> Option<&str> {
        match self {
            Self::Http { name, .. } => name.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_error_from_named_error() {
        let body = r#"{"name":"NotFound","message":"Session not found","data":{"id":"123"}}"#;
        let err = OpencodeError::http(404, body);

        match err {
            OpencodeError::Http {
                status,
                name,
                message,
                data,
            } => {
                assert_eq!(status, 404);
                assert_eq!(name, Some("NotFound".to_string()));
                assert_eq!(message, "Session not found");
                assert!(data.is_some());
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_http_error_from_plain_text() {
        let err = OpencodeError::http(500, "Internal Server Error");

        match err {
            OpencodeError::Http {
                status,
                name,
                message,
                ..
            } => {
                assert_eq!(status, 500);
                assert!(name.is_none());
                assert_eq!(message, "HTTP 500");
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_is_not_found() {
        let err = OpencodeError::http(404, "{}");
        assert!(err.is_not_found());

        let err = OpencodeError::http(200, "{}");
        assert!(!err.is_not_found());
    }

    #[test]
    fn test_is_validation_error() {
        let err = OpencodeError::http(400, r#"{"name":"ValidationError"}"#);
        assert!(err.is_validation_error());
        assert_eq!(err.error_name(), Some("ValidationError"));
    }

    #[test]
    fn test_is_server_error() {
        let err = OpencodeError::http(500, "{}");
        assert!(err.is_server_error());

        let err = OpencodeError::http(503, "{}");
        assert!(err.is_server_error());

        let err = OpencodeError::http(400, "{}");
        assert!(!err.is_server_error());
    }
}
