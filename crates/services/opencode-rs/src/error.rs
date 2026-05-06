//! Error types for `opencode_rs`.

use thiserror::Error;

/// Result type alias for `opencode_rs` operations.
pub type Result<T> = std::result::Result<T, OpencodeError>;

/// Error type for `opencode_rs` operations.
#[derive(Debug, Error)]
pub enum OpencodeError {
    /// HTTP error with structured response from `OpenCode`.
    #[error("HTTP error {status}: {message}")]
    Http {
        /// HTTP status code.
        status: u16,
        /// Error name from `OpenCode`'s `NamedError` (e.g., "`NotFound`", "`ValidationError`").
        name: Option<String>,
        /// Error message.
        message: String,
        /// Additional error data.
        data: Option<serde_json::Value>,
    },

    /// Transport/network error.
    #[error("Transport error: {0}")]
    Transport(#[from] reqwest::Error),

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

    /// Version mismatch between SDK expectation and server response.
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch {
        /// Expected version.
        expected: String,
        /// Actual version.
        actual: String,
    },
}

/// Helper to parse `OpenCode`'s `NamedError` response body.
#[derive(Debug, Clone)]
pub struct HttpErrorBody {
    /// Error name (e.g., "`NotFound`", "`ValidationError`").
    pub name: Option<String>,
    /// Error message.
    pub message: Option<String>,
    /// Additional error data.
    pub data: Option<serde_json::Value>,
}

fn format_error_path(path: &[serde_json::Value]) -> Option<String> {
    let formatted: Vec<String> = path
        .iter()
        .map(|segment| match segment {
            serde_json::Value::String(value) => value.clone(),
            other => other.to_string(),
        })
        .collect();

    if formatted.is_empty() {
        None
    } else {
        Some(formatted.join("."))
    }
}

fn format_validator_entry(entry: &serde_json::Value) -> String {
    let path = entry
        .get("path")
        .and_then(serde_json::Value::as_array)
        .and_then(|segments| format_error_path(segments));
    let message = entry
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(std::string::ToString::to_string);

    match (path, message) {
        (Some(path), Some(message)) => format!("{path}: {message}"),
        (Some(path), None) => format!("{path}: {entry}"),
        (None, Some(message)) => message,
        (None, None) => entry.to_string(),
    }
}

fn extract_http_error_message(v: &serde_json::Value) -> Option<String> {
    if matches!(v.get("success"), Some(serde_json::Value::Bool(false)))
        && let Some(errors) = v.get("error").and_then(serde_json::Value::as_array)
    {
        let messages: Vec<String> = errors.iter().map(format_validator_entry).collect();
        if !messages.is_empty() {
            return Some(messages.join("; "));
        }
    }

    if let (Some(name), Some(message)) = (
        v.get("name").and_then(serde_json::Value::as_str),
        v.get("data")
            .and_then(|data| data.get("message"))
            .and_then(serde_json::Value::as_str),
    ) {
        return Some(format!("{name}: {message}"));
    }

    v.get("message")
        .and_then(serde_json::Value::as_str)
        .map(std::string::ToString::to_string)
}

fn truncate_body_text(body_text: &str, max_chars: usize) -> String {
    match body_text.char_indices().nth(max_chars) {
        Some((idx, _)) => format!("{}…", &body_text[..idx]),
        None => body_text.to_string(),
    }
}

impl HttpErrorBody {
    /// Parse from a JSON value.
    pub fn from_json(v: &serde_json::Value) -> Self {
        Self {
            name: v
                .get("name")
                .and_then(|x| x.as_str())
                .map(std::string::ToString::to_string),
            message: extract_http_error_message(v),
            data: v.get("data").cloned(),
        }
    }
}

impl OpencodeError {
    /// Create an HTTP error from status and optional JSON body.
    pub fn http(status: u16, body_text: &str) -> Self {
        let parsed: Option<serde_json::Value> = serde_json::from_str(body_text).ok();
        let info = parsed.as_ref().map(HttpErrorBody::from_json);
        let message = info
            .as_ref()
            .and_then(|i| i.message.clone())
            .unwrap_or_else(|| {
                let truncated = truncate_body_text(body_text, 1024);
                if truncated.trim().is_empty() {
                    format!("HTTP {status}")
                } else {
                    truncated
                }
            });

        Self::Http {
            status,
            name: info.as_ref().and_then(|i| i.name.clone()),
            message,
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
        let body = r#"{"name":"NotFound","data":{"message":"Session not found","id":"123"}}"#;
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
                assert_eq!(message, "NotFound: Session not found");
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
                assert_eq!(message, "Internal Server Error");
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_http_error_from_empty_body_uses_status_fallback() {
        let err = OpencodeError::http(502, "");

        match err {
            OpencodeError::Http {
                status,
                name,
                message,
                ..
            } => {
                assert_eq!(status, 502);
                assert!(name.is_none());
                assert_eq!(message, "HTTP 502");
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_http_error_from_whitespace_body_uses_status_fallback() {
        let err = OpencodeError::http(503, "   \n");

        match err {
            OpencodeError::Http {
                status,
                name,
                message,
                ..
            } => {
                assert_eq!(status, 503);
                assert!(name.is_none());
                assert_eq!(message, "HTTP 503");
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_http_error_from_legacy_top_level_message() {
        let body = r#"{"name":"ValidationError","message":"Legacy message"}"#;
        let err = OpencodeError::http(400, body);

        match err {
            OpencodeError::Http { name, message, .. } => {
                assert_eq!(name, Some("ValidationError".to_string()));
                assert_eq!(message, "Legacy message");
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_http_error_from_validator_messageid_invalid() {
        let body = r#"{
  "data": {"command":"linear","arguments":"hello","messageID":"550e8400-e29b-41d4-a716-446655440000"},
  "error": [
    {
      "origin": "string",
      "code": "invalid_format",
      "format": "starts_with",
      "prefix": "msg",
      "path": ["messageID"],
      "message": "Invalid string: must start with \"msg\""
    }
  ],
  "success": false
}"#;
        let err = OpencodeError::http(400, body);

        match err {
            OpencodeError::Http { message, .. } => {
                assert!(message.contains("messageID"), "message was: {message}");
                assert!(
                    message.contains("must start with"),
                    "message was: {message}"
                );
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_http_error_from_named_error_unknown_command() {
        let body = r#"{
  "name": "UnknownError",
  "data": {
    "message": "Command not found: \"___definitely_not_a_real_command___\". Available commands: init, review, ..."
  }
}"#;
        let err = OpencodeError::http(400, body);

        match err {
            OpencodeError::Http { message, .. } => {
                assert!(
                    message.contains("Command not found"),
                    "message was: {message}"
                );
                assert!(
                    message.contains("___definitely_not_a_real_command___"),
                    "message was: {message}"
                );
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[test]
    fn test_http_error_from_unknown_shape_preserves_body() {
        let body = r#"{ "weird": "shape" }"#;
        let err = OpencodeError::http(418, body);

        match err {
            OpencodeError::Http { message, .. } => {
                assert!(message.contains(body), "message was: {message}");
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
