//! Worktree API for OpenCode.
//!
//! Experimental endpoints for git worktree management.

use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;
use serde::{Deserialize, Serialize};

/// Worktree API client.
#[derive(Clone)]
pub struct WorktreeApi {
    http: HttpClient,
}

impl WorktreeApi {
    /// Create a new Worktree API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Create a worktree (experimental).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn create(&self, req: &CreateWorktreeRequest) -> Result<Worktree> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/experimental/worktree", Some(body))
            .await
    }

    /// List worktrees (experimental).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<Worktree>> {
        self.http
            .request_json(Method::GET, "/experimental/worktree", None)
            .await
    }
}

/// Request to create a worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorktreeRequest {
    /// Branch name.
    pub branch: String,
    /// Path for the worktree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// A git worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// Worktree path.
    pub path: String,
    /// Branch name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Whether this is the main worktree.
    #[serde(default)]
    pub is_main: bool,
}
