//! Worktree API for `OpenCode`.

use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;
use serde::Deserialize;
use serde::Serialize;

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

    /// Create a worktree.
    pub async fn create(&self, req: &WorktreeCreateInput) -> Result<Worktree> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/experimental/worktree", Some(body))
            .await
    }

    /// List worktree sandbox directories.
    pub async fn list(&self) -> Result<Vec<String>> {
        self.http
            .request_json(Method::GET, "/experimental/worktree", None)
            .await
    }

    /// Remove a worktree.
    pub async fn remove(&self, req: &WorktreeRemoveInput) -> Result<bool> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::DELETE, "/experimental/worktree", Some(body))
            .await
    }

    /// Reset a worktree.
    pub async fn reset(&self, req: &WorktreeResetInput) -> Result<bool> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/experimental/worktree/reset", Some(body))
            .await
    }
}

/// Worktree creation input.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCreateInput {
    pub name: Option<String>,
    pub start_command: Option<String>,
}

/// Worktree info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worktree {
    pub name: String,
    pub branch: String,
    pub directory: String,
}

/// Worktree remove input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRemoveInput {
    pub directory: String,
}

/// Worktree reset input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeResetInput {
    pub directory: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_json;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn test_worktree_create() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/experimental/worktree"))
            .and(body_json(
                serde_json::json!({"name": "feature-a", "startCommand": "pnpm dev"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "feature-a",
                "branch": "feature-a",
                "directory": "/tmp/worktrees/feature-a"
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = WorktreeApi::new(client);
        let worktree = api
            .create(&WorktreeCreateInput {
                name: Some("feature-a".to_string()),
                start_command: Some("pnpm dev".to_string()),
            })
            .await
            .unwrap();
        assert_eq!(worktree.directory, "/tmp/worktrees/feature-a");
    }

    #[tokio::test]
    async fn test_worktree_list() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/experimental/worktree"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                "/tmp/worktrees/feature-a",
                "/tmp/worktrees/feature-b"
            ])))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = WorktreeApi::new(client);
        let worktrees = api.list().await.unwrap();
        assert_eq!(worktrees.len(), 2);
    }

    #[tokio::test]
    async fn test_worktree_remove() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/experimental/worktree"))
            .and(body_json(
                serde_json::json!({"directory": "/tmp/worktrees/feature-a"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = WorktreeApi::new(client);
        let removed = api
            .remove(&WorktreeRemoveInput {
                directory: "/tmp/worktrees/feature-a".to_string(),
            })
            .await
            .unwrap();
        assert!(removed);
    }

    #[tokio::test]
    async fn test_worktree_reset() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/experimental/worktree/reset"))
            .and(body_json(
                serde_json::json!({"directory": "/tmp/worktrees/feature-a"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = WorktreeApi::new(client);
        let reset = api
            .reset(&WorktreeResetInput {
                directory: "/tmp/worktrees/feature-a".to_string(),
            })
            .await
            .unwrap();
        assert!(reset);
    }
}
