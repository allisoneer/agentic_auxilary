//! Find API for OpenCode.
//!
//! Endpoints for searching files and symbols.

use crate::error::Result;
use crate::http::HttpClient;
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

    /// Search for text in files.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn text(&self, query: &str) -> Result<serde_json::Value> {
        let encoded = urlencoding::encode(query);
        self.http
            .request_json(Method::GET, &format!("/find?query={}", encoded), None)
            .await
    }

    /// Search for files by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn files(&self, query: &str) -> Result<serde_json::Value> {
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
    pub async fn symbols(&self, query: &str) -> Result<serde_json::Value> {
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
