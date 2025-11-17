use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when using the Anthropic API client
#[derive(Debug, Error)]
pub enum AnthropicError {
    /// HTTP request error
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// API error returned by Anthropic
    #[error("API error: {0:?}")]
    Api(ApiErrorObject),

    /// Configuration error (e.g., missing credentials)
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serde(String),
}

/// API error object from Anthropic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorObject {
    /// Error type (e.g., "`invalid_request_error`", "`rate_limit_error`")
    pub r#type: Option<String>,
    /// Human-readable error message
    pub message: String,
    /// Request ID for debugging
    pub request_id: Option<String>,
    /// Error code
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorEnvelope {
    error: ApiErrorObject,
}

/// Maps a serde deserialization error to an `AnthropicError` with context
///
/// Includes a snippet of the response body for debugging.
#[must_use]
pub fn map_deser(e: &serde_json::Error, body: &[u8]) -> AnthropicError {
    let snippet = String::from_utf8_lossy(&body[..body.len().min(400)]).to_string();
    AnthropicError::Serde(format!("{e}: {snippet}"))
}

/// Deserializes an API error from the response body
///
/// Attempts to parse the error as JSON, falling back to plain text on failure.
#[must_use]
pub fn deserialize_api_error(status: StatusCode, body: &[u8]) -> AnthropicError {
    if let Ok(envelope) = serde_json::from_slice::<ErrorEnvelope>(body) {
        return AnthropicError::Api(envelope.error);
    }

    // Server may return plain text on 5xx
    AnthropicError::Api(ApiErrorObject {
        r#type: Some(format!("http_{}", status.as_u16())),
        message: String::from_utf8_lossy(body).into_owned(),
        request_id: None,
        code: None,
    })
}
