//! Neutral HTTP error and status classifiers.
//!
//! This module provides utilities for classifying HTTP responses
//! in a consistent, transport-agnostic way.

use reqwest::{Error as ReqwestError, StatusCode};

/// Classification of HTTP errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpKind {
    /// 401 Unauthorized
    Unauthorized,
    /// 403 Forbidden
    Forbidden,
    /// 404 Not Found
    NotFound,
    /// 429 Too Many Requests
    RateLimited,
    /// Other 4xx errors
    ClientError,
    /// 5xx errors
    ServerError,
    /// Request timeout
    Timeout,
    /// Network/connection error
    Network,
    /// Unknown or unclassified error
    Unknown,
}

/// Summary of an HTTP error for consistent handling.
#[derive(Debug, Clone)]
pub struct HttpErrorSummary {
    /// The classified error kind
    pub kind: HttpKind,
    /// HTTP status code if available
    pub status: Option<u16>,
    /// Human-readable error message
    pub message: String,
}

/// Classify an HTTP status code.
pub fn classify_status(status: StatusCode) -> HttpKind {
    match status {
        StatusCode::UNAUTHORIZED => HttpKind::Unauthorized,
        StatusCode::FORBIDDEN => HttpKind::Forbidden,
        StatusCode::NOT_FOUND => HttpKind::NotFound,
        StatusCode::TOO_MANY_REQUESTS => HttpKind::RateLimited,
        s if s.is_client_error() => HttpKind::ClientError,
        s if s.is_server_error() => HttpKind::ServerError,
        _ => HttpKind::Unknown,
    }
}

/// Summarize a reqwest error into a consistent format.
pub fn summarize_reqwest_error(err: &ReqwestError) -> HttpErrorSummary {
    if err.is_timeout() {
        return HttpErrorSummary {
            kind: HttpKind::Timeout,
            status: None,
            message: err.to_string(),
        };
    }

    if err.is_connect() {
        return HttpErrorSummary {
            kind: HttpKind::Network,
            status: None,
            message: err.to_string(),
        };
    }

    if let Some(status) = err.status() {
        let kind = classify_status(status);
        return HttpErrorSummary {
            kind,
            status: Some(status.as_u16()),
            message: err.to_string(),
        };
    }

    HttpErrorSummary {
        kind: HttpKind::Unknown,
        status: None,
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_status_unauthorized() {
        assert_eq!(
            classify_status(StatusCode::UNAUTHORIZED),
            HttpKind::Unauthorized
        );
    }

    #[test]
    fn classify_status_forbidden() {
        assert_eq!(classify_status(StatusCode::FORBIDDEN), HttpKind::Forbidden);
    }

    #[test]
    fn classify_status_not_found() {
        assert_eq!(classify_status(StatusCode::NOT_FOUND), HttpKind::NotFound);
    }

    #[test]
    fn classify_status_rate_limited() {
        assert_eq!(
            classify_status(StatusCode::TOO_MANY_REQUESTS),
            HttpKind::RateLimited
        );
    }

    #[test]
    fn classify_status_other_client_error() {
        assert_eq!(
            classify_status(StatusCode::BAD_REQUEST),
            HttpKind::ClientError
        );
        assert_eq!(classify_status(StatusCode::CONFLICT), HttpKind::ClientError);
    }

    #[test]
    fn classify_status_server_error() {
        assert_eq!(
            classify_status(StatusCode::INTERNAL_SERVER_ERROR),
            HttpKind::ServerError
        );
        assert_eq!(
            classify_status(StatusCode::BAD_GATEWAY),
            HttpKind::ServerError
        );
        assert_eq!(
            classify_status(StatusCode::SERVICE_UNAVAILABLE),
            HttpKind::ServerError
        );
    }

    #[test]
    fn classify_status_ok_is_unknown() {
        // Success codes aren't really "errors" so they classify as Unknown
        assert_eq!(classify_status(StatusCode::OK), HttpKind::Unknown);
    }
}
