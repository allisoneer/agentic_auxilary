use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when using the Exa API client
#[derive(Debug, Error)]
pub enum ExaError {
    /// HTTP request error
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// API error returned by Exa
    #[error("API error: {0:?}")]
    Api(ApiErrorObject),

    /// Configuration error (e.g., missing credentials)
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serde(String),
}

/// API error object from Exa
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorObject {
    /// HTTP status code
    #[serde(default)]
    pub status_code: Option<u16>,
    /// Human-readable error message
    #[serde(default)]
    pub message: String,
    /// Timestamp of the error
    #[serde(default)]
    pub timestamp: Option<String>,
    /// Request path
    #[serde(default)]
    pub path: Option<String>,
    /// Error type string
    #[serde(default)]
    pub error: Option<String>,
}

impl ExaError {
    /// Determines if this error is retryable
    ///
    /// Retryable errors include rate limits (429), timeouts (408),
    /// and server errors (5xx).
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Api(obj) => obj
                .status_code
                .is_some_and(crate::retry::is_retryable_status),
            Self::Reqwest(e) => e.is_timeout() || e.is_connect(),
            Self::Config(_) | Self::Serde(_) => false,
        }
    }
}

/// Maps a serde deserialization error to an `ExaError` with context
#[must_use]
pub fn map_deser(e: &serde_json::Error, body: &[u8]) -> ExaError {
    let snippet = String::from_utf8_lossy(&body[..body.len().min(400)]).to_string();
    ExaError::Serde(format!("{e}: {snippet}"))
}

/// Deserializes an API error from the response body
///
/// Attempts to parse the error as JSON, falling back to plain text on failure.
#[must_use]
pub fn deserialize_api_error(status: StatusCode, body: &[u8]) -> ExaError {
    let status_code = Some(status.as_u16());

    if let Ok(mut obj) = serde_json::from_slice::<ApiErrorObject>(body) {
        obj.status_code = status_code;
        return ExaError::Api(obj);
    }

    // Server may return plain text on 5xx; cap body to avoid log/memory bloat
    ExaError::Api(ApiErrorObject {
        status_code,
        message: String::from_utf8_lossy(&body[..body.len().min(400)]).into_owned(),
        timestamp: None,
        path: None,
        error: Some(format!("http_{}", status.as_u16())),
    })
}
