//! Claude Core - ChatController and streaming infrastructure

pub mod amortize;
pub mod async_utils;
pub mod controller;
pub mod database;
pub mod plugin;
pub mod repository;
pub mod snapshot;
pub mod state;
pub mod vec_mutation;

/// Default model to use for new conversations
pub const DEFAULT_MODEL: &str = "claude-sonnet-4-5-20250929";

/// Available models with their display names
pub const AVAILABLE_MODELS: &[(&str, &str)] = &[
    ("claude-sonnet-4-5-20250929", "Claude Sonnet 4.5"),
    ("claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
    ("claude-opus-4-5-20251101", "Claude Opus 4.5"),
];

/// Core error type
#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    /// Authentication error
    #[error("Auth error: {0}")]
    Auth(String),
    /// API error
    #[error("API error: {0}")]
    Api(String),
    /// Database error
    #[error("Database error: {0}")]
    Db(String),
    /// Other error
    #[error("Other: {0}")]
    Other(String),
}
