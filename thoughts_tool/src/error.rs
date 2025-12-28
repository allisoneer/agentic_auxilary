use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ThoughtsError {
    #[error("Not in a git repository")]
    NotInGitRepo,

    #[error("Mount source does not exist: {path}")]
    MountSourceNotFound { path: PathBuf },

    #[error("Platform not supported: {platform}")]
    PlatformNotSupported { platform: String },

    #[error("Required tool not found: {tool}")]
    ToolNotFound { tool: String },

    #[error("Mount operation failed: {message}")]
    MountOperationFailed { message: String },

    #[error("Mount verification timed out: target {target} not visible after {timeout_secs}s")]
    MountVerificationTimeout { target: PathBuf, timeout_secs: u64 },

    #[error("Command timeout: {command} timed out after {timeout_secs} seconds")]
    CommandTimeout { command: String, timeout_secs: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Git2 error: {0}")]
    Git2(#[from] git2::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ThoughtsError>;
