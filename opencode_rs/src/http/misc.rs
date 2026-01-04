//! Miscellaneous API endpoints for OpenCode.
//!
//! Includes: VCS, path, instance, log, LSP, formatter, global endpoints.

use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;
use serde::{Deserialize, Serialize};

/// Misc API client.
#[derive(Clone)]
pub struct MiscApi {
    http: HttpClient,
}

impl MiscApi {
    /// Create a new Misc API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Get current path info.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn path(&self) -> Result<PathInfo> {
        self.http.request_json(Method::GET, "/path", None).await
    }

    /// Get VCS info.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn vcs(&self) -> Result<VcsInfo> {
        self.http.request_json(Method::GET, "/vcs", None).await
    }

    /// Dispose instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn dispose(&self) -> Result<()> {
        self.http
            .request_empty(
                Method::POST,
                "/instance/dispose",
                Some(serde_json::json!({})),
            )
            .await
    }

    /// Write log entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn log(&self, entry: &LogEntry) -> Result<()> {
        let body = serde_json::to_value(entry)?;
        self.http
            .request_empty(Method::POST, "/log", Some(body))
            .await
    }

    /// Get LSP status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn lsp(&self) -> Result<serde_json::Value> {
        self.http.request_json(Method::GET, "/lsp", None).await
    }

    /// Get formatter status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn formatter(&self) -> Result<serde_json::Value> {
        self.http
            .request_json(Method::GET, "/formatter", None)
            .await
    }

    /// Get global health.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn health(&self) -> Result<HealthInfo> {
        self.http
            .request_json(Method::GET, "/global/health", None)
            .await
    }

    /// Dispose global.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn global_dispose(&self) -> Result<()> {
        self.http
            .request_empty(Method::POST, "/global/dispose", Some(serde_json::json!({})))
            .await
    }

    /// Get OpenAPI spec.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn doc(&self) -> Result<serde_json::Value> {
        self.http.request_json(Method::GET, "/doc", None).await
    }
}

/// Path information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    /// Current directory.
    pub directory: String,
    /// Project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_root: Option<String>,
}

/// VCS information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsInfo {
    /// VCS type (git, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Current branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Remote URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

/// Log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Log level.
    pub level: String,
    /// Log message.
    pub message: String,
    /// Additional data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Health information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthInfo {
    /// Whether healthy.
    pub healthy: bool,
    /// Server version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
