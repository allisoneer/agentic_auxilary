//! Find API for OpenCode.
//!
//! Endpoints for searching files and symbols.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::api::FindResponse;
use reqwest::Method;

/// Find API client.
#[derive(Clone)]
pub struct FindApi {
    http: HttpClient,
}

impl FindApi {
    /// Create a new Find API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Search for text in files using ripgrep.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn text(&self, pattern: &str) -> Result<FindResponse> {
        let encoded = urlencoding::encode(pattern);
        self.http
            .request_json(Method::GET, &format!("/find?pattern={}", encoded), None)
            .await
    }

    /// Search for files by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn files(&self, query: &str) -> Result<FindResponse> {
        let encoded = urlencoding::encode(query);
        self.http
            .request_json(Method::GET, &format!("/find/file?query={}", encoded), None)
            .await
    }

    /// Search for symbols.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn symbols(&self, query: &str) -> Result<FindResponse> {
        let encoded = urlencoding::encode(query);
        self.http
            .request_json(
                Method::GET,
                &format!("/find/symbol?query={}", encoded),
                None,
            )
            .await
    }
}
