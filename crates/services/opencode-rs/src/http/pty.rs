//! PTY API for OpenCode.
//!
//! Endpoints for pseudo-terminal management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::pty::{CreatePtyRequest, Pty, UpdatePtyRequest};
use reqwest::Method;

/// PTY API client.
#[derive(Clone)]
pub struct PtyApi {
    http: HttpClient,
}

impl PtyApi {
    /// Create a new PTY API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List PTYs.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<Pty>> {
        self.http.request_json(Method::GET, "/pty", None).await
    }

    /// Create a new PTY.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn create(&self, req: &CreatePtyRequest) -> Result<Pty> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/pty", Some(body))
            .await
    }

    /// Get a PTY by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get(&self, id: &str) -> Result<Pty> {
        self.http
            .request_json(Method::GET, &format!("/pty/{}", id), None)
            .await
    }

    /// Update a PTY.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn update(&self, id: &str, req: &UpdatePtyRequest) -> Result<Pty> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::PUT, &format!("/pty/{}", id), Some(body))
            .await
    }

    /// Delete a PTY.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.http
            .request_empty(Method::DELETE, &format!("/pty/{}", id), None)
            .await
    }
}
