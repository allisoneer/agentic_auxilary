//! API error types for opencode_rs.
//!
//! Contains typed error structures matching TypeScript MessageV2.APIError.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// API error from the OpenCode server.
///
/// Matches the TypeScript `MessageV2.APIError` schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct APIError {
    /// Error message.
    pub message: String,
    /// HTTP status code if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    /// Whether this error is retryable.
    pub is_retryable: bool,
    /// Response headers from the failed request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<HashMap<String, String>>,
    /// Response body from the failed request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    /// Additional error metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

impl std::fmt::Display for APIError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(code) = self.status_code {
            write!(f, " (status: {})", code)?;
        }
        Ok(())
    }
}

impl std::error::Error for APIError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_minimal() {
        let json = r#"{"message":"Something went wrong","isRetryable":false}"#;
        let error: APIError = serde_json::from_str(json).unwrap();
        assert_eq!(error.message, "Something went wrong");
        assert!(!error.is_retryable);
        assert!(error.status_code.is_none());
    }

    #[test]
    fn test_api_error_full() {
        let json = r#"{
            "message": "Rate limited",
            "statusCode": 429,
            "isRetryable": true,
            "responseHeaders": {"retry-after": "60"},
            "responseBody": "Too many requests",
            "metadata": {"region": "us-east-1"}
        }"#;
        let error: APIError = serde_json::from_str(json).unwrap();
        assert_eq!(error.message, "Rate limited");
        assert_eq!(error.status_code, Some(429));
        assert!(error.is_retryable);
        assert_eq!(
            error.response_headers.as_ref().unwrap().get("retry-after"),
            Some(&"60".to_string())
        );
        assert_eq!(error.response_body, Some("Too many requests".to_string()));
        assert_eq!(
            error.metadata.as_ref().unwrap().get("region"),
            Some(&"us-east-1".to_string())
        );
    }

    #[test]
    fn test_api_error_display() {
        let error = APIError {
            message: "Not found".to_string(),
            status_code: Some(404),
            is_retryable: false,
            response_headers: None,
            response_body: None,
            metadata: None,
        };
        assert_eq!(error.to_string(), "Not found (status: 404)");
    }

    #[test]
    fn test_api_error_roundtrip() {
        let error = APIError {
            message: "Test error".to_string(),
            status_code: Some(500),
            is_retryable: true,
            response_headers: Some(HashMap::from([(
                "x-request-id".to_string(),
                "123".to_string(),
            )])),
            response_body: Some("Internal error".to_string()),
            metadata: None,
        };
        let json = serde_json::to_string(&error).unwrap();
        let parsed: APIError = serde_json::from_str(&json).unwrap();
        assert_eq!(error, parsed);
    }
}
