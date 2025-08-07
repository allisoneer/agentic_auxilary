use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use thiserror::Error;
use std::collections::HashMap;

/// The primary error type that all tool functions must return in their Result.
#[derive(Debug, Error, Serialize, Deserialize, JsonSchema)]
pub struct ToolError {
    /// A machine-readable code for the error category.
    pub code: ErrorCode,
    /// A human-readable, context-specific error message.
    pub message: String,
    /// Additional error context as key-value pairs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
}

/// A controlled vocabulary for error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ErrorCode {
    BadRequest,
    InvalidArgument,
    NotFound,
    PermissionDenied,
    Internal,
    Timeout,
    Conflict,
    NetworkError,
    ExternalServiceError,
    ExecutionFailed,
    SerializationError,
    IoError,
}

// Implementation of Display for ToolError to satisfy the Error trait
impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

// Convenience constructors
impl ToolError {
    /// Create a new ToolError with the given code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Add details to this error.
    pub fn with_details(mut self, details: HashMap<String, serde_json::Value>) -> Self {
        self.details = Some(details);
        self
    }
    
    /// Add a single detail string to this error.
    pub fn with_detail(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        let mut details = self.details.unwrap_or_default();
        details.insert(key.to_string(), value.into());
        self.details = Some(details);
        self
    }

    // Convenience constructors for common error types
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }
}

// From implementations for common error types
impl From<std::io::Error> for ToolError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorCode::ExecutionFailed, err.to_string())
    }
}

impl From<serde_json::Error> for ToolError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(ErrorCode::InvalidArgument, err.to_string())
    }
}

impl From<String> for ToolError {
    fn from(err: String) -> Self {
        Self::new(ErrorCode::ExecutionFailed, err)
    }
}

impl From<&str> for ToolError {
    fn from(err: &str) -> Self {
        Self::new(ErrorCode::ExecutionFailed, err)
    }
}