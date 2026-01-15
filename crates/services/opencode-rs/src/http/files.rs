//! Files API for OpenCode.
//!
//! Endpoints for file operations.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::file::{FileContent, FileInfo, FileStatus};
use reqwest::Method;

/// Files API client.
#[derive(Clone)]
pub struct FilesApi {
    http: HttpClient,
}

impl FilesApi {
    /// Create a new Files API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List files in the project.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<FileInfo>> {
        self.http.request_json(Method::GET, "/file", None).await
    }

    /// Read file content.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn read(&self, path: &str) -> Result<FileContent> {
        let encoded = urlencoding::encode(path);
        self.http
            .request_json(
                Method::GET,
                &format!("/file/content?path={}", encoded),
                None,
            )
            .await
    }

    /// Get file VCS status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn status(&self) -> Result<Vec<FileStatus>> {
        self.http
            .request_json(Method::GET, "/file/status", None)
            .await
    }
}
