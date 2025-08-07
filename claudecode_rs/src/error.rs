use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClaudeError {
    #[error("Claude executable not found in PATH")]
    ClaudeNotFound,

    #[error("Claude executable not found at path: {path}")]
    ClaudeNotFoundAtPath { path: PathBuf },

    #[error("Invalid configuration: {message}")]
    InvalidConfiguration { message: String },

    #[error("Failed to spawn process '{command}': {source}")]
    SpawnError {
        command: String,
        args: Vec<String>,
        #[source]
        source: std::io::Error,
    },

    #[error("Process exited with code {code}: {stderr}")]
    ProcessFailed { code: i32, stderr: String },

    #[error("Failed to parse JSON: {source}")]
    JsonParseError {
        #[source]
        source: serde_json::Error,
        line: Option<String>,
    },

    #[error("Stream closed unexpectedly")]
    StreamClosed,

    #[error("Session error: {message}")]
    SessionError { message: String },

    #[error("IO error: {source}")]
    IoError {
        #[from]
        source: std::io::Error,
    },
}

impl From<serde_json::Error> for ClaudeError {
    fn from(err: serde_json::Error) -> Self {
        ClaudeError::JsonParseError {
            source: err,
            line: None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ClaudeError>;
