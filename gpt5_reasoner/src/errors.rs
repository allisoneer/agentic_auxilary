use std::path::PathBuf;
use thiserror::Error;
use universal_tool_core::error::{ErrorCode, ToolError};

pub type Result<T> = std::result::Result<T, ReasonerError>;

#[derive(Debug, Error)]
pub enum ReasonerError {
    #[error("Missing environment variable: {0}")]
    MissingEnv(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("XML error: {0}")]
    Xml(String), // quick-xml errors not always stable types; wrap as string

    #[error("OpenAI client error: {0}")]
    OpenAI(#[from] async_openai::error::OpenAIError),

    #[error("Template validation error: {0}")]
    Template(String),

    #[error("Token limit exceeded: {current} > {limit}")]
    TokenLimit { current: usize, limit: usize },

    #[error("Unsupported file encoding (non-UTF8): {0}")]
    NonUtf8(PathBuf),

    #[error("File not found: {0}")]
    MissingFile(PathBuf),
}

impl From<ReasonerError> for ToolError {
    fn from(e: ReasonerError) -> Self {
        match &e {
            ReasonerError::MissingEnv(_) => {
                ToolError::new(ErrorCode::InvalidArgument, e.to_string())
            }
            ReasonerError::MissingFile(_) => ToolError::new(ErrorCode::NotFound, e.to_string()),
            ReasonerError::NonUtf8(_) => ToolError::new(ErrorCode::InvalidArgument, e.to_string()),
            ReasonerError::TokenLimit { .. } => {
                ToolError::new(ErrorCode::InvalidArgument, e.to_string())
            }
            ReasonerError::Template(_)
            | ReasonerError::Yaml(_)
            | ReasonerError::Xml(_)
            | ReasonerError::Json(_) => ToolError::new(ErrorCode::InvalidArgument, e.to_string()),
            ReasonerError::OpenAI(_) => {
                ToolError::new(ErrorCode::ExternalServiceError, e.to_string())
            }
            ReasonerError::Io(_) => ToolError::new(ErrorCode::IoError, e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_env_to_tool_error() {
        let err = ReasonerError::MissingEnv("TEST_VAR".into());
        let tool_err: ToolError = err.into();
        // Can't directly check ErrorCode since it's not public, but we can check the conversion works
        assert!(tool_err.to_string().contains("TEST_VAR"));
    }
}
