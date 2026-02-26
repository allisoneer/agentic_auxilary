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

    /// Delete a worktree (experimental).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete(&self, req: &DeleteWorktreeRequest) -> Result<()> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_empty(Method::DELETE, "/experimental/worktree", Some(body))
            .await
    }

    /// Reset worktree state (experimental).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn reset(&self, req: &ResetWorktreeRequest) -> Result<WorktreeResetResponse> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/experimental/worktree/reset", Some(body))
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

/// Request to delete a worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorktreeRequest {
    /// Path of the worktree to delete.
    pub path: String,
    /// Force deletion even if there are uncommitted changes.
    #[serde(default)]
    pub force: bool,
}

/// Request to reset a worktree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetWorktreeRequest {
    /// Path of the worktree to reset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Reset mode (soft, mixed, hard).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Target ref to reset to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Response from worktree reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeResetResponse {
    /// Whether the reset was successful.
    #[serde(default)]
    pub success: bool,
    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_worktree_delete() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/experimental/worktree"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = WorktreeApi::new(client);
        let req = DeleteWorktreeRequest {
            path: "/path/to/worktree".to_string(),
            force: false,
        };
        api.delete(&req).await.unwrap();
    }

    #[tokio::test]
    async fn test_worktree_reset() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/experimental/worktree/reset"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = WorktreeApi::new(client);
        let req = ResetWorktreeRequest {
            path: Some("/path/to/worktree".to_string()),
            mode: Some("hard".to_string()),
            target: Some("HEAD~1".to_string()),
        };
        let response = api.reset(&req).await.unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_delete_worktree_request() {
        let req = DeleteWorktreeRequest {
            path: "/tmp/worktree".to_string(),
            force: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("force"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_reset_worktree_request() {
        let req = ResetWorktreeRequest {
            path: None,
            mode: Some("soft".to_string()),
            target: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("soft"));
    }
}
