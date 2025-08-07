//! REST API utilities for the Universal Tool Framework
//!
//! This module provides utilities for REST API applications including
//! re-exports of axum and related dependencies.

// Re-export axum and related types so users don't need to depend on them
pub use axum;
pub use axum::*;

// Re-export tower utilities commonly used with axum
pub use tower;
pub use tower_http;

// Re-export utoipa for OpenAPI support when the openapi feature is enabled
#[cfg(feature = "openapi")]
pub use utoipa;
#[cfg(feature = "openapi")]
pub use utoipa_swagger_ui;

// Re-export common response/request types
pub use axum::response::{IntoResponse, Response};
pub use axum::extract::{Json, Path, Query, State};
pub use axum::http::StatusCode;