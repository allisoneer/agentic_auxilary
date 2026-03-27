//! Snapshots API for `OpenCode`.
//!
//! Endpoints for managing file snapshots and diffs.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::snapshot::FileDiff;
use crate::types::snapshot::FilePatch;
use crate::types::snapshot::SnapshotDiffRequest;
use crate::types::snapshot::SnapshotDiffResponse;
use crate::types::snapshot::SnapshotPatchRequest;
use crate::types::snapshot::SnapshotPatchResponse;
use crate::types::snapshot::SnapshotRestoreRequest;
use crate::types::snapshot::SnapshotRestoreResponse;
use crate::types::snapshot::SnapshotRevertRequest;
use crate::types::snapshot::SnapshotRevertResponse;
use crate::types::snapshot::SnapshotTrackRequest;
use crate::types::snapshot::SnapshotTrackResponse;
use reqwest::Method;

/// Snapshots API client.
#[derive(Clone)]
pub struct SnapshotsApi {
    http: HttpClient,
}

impl SnapshotsApi {
    /// Create a new Snapshots API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Track files for snapshotting.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn track(&self, request: &SnapshotTrackRequest) -> Result<SnapshotTrackResponse> {
        let body = serde_json::to_value(request)?;
        self.http
            .request_json(Method::POST, "/snapshot/track", Some(body))
            .await
    }

    /// Get patches for tracked files.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn patch(&self, request: &SnapshotPatchRequest) -> Result<SnapshotPatchResponse> {
        let body = serde_json::to_value(request)?;
        self.http
            .request_json(Method::POST, "/snapshot/patch", Some(body))
            .await
    }

    /// Get patches as a flat list.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn patch_list(&self, request: &SnapshotPatchRequest) -> Result<Vec<FilePatch>> {
        let response = self.patch(request).await?;
        Ok(response.patches)
    }

    /// Get diffs for tracked files.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn diff(&self, request: &SnapshotDiffRequest) -> Result<SnapshotDiffResponse> {
        let body = serde_json::to_value(request)?;
        self.http
            .request_json(Method::POST, "/snapshot/diff", Some(body))
            .await
    }

    /// Get full diffs with content.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn diff_full(&self, request: &SnapshotDiffRequest) -> Result<Vec<FileDiff>> {
        let body = serde_json::to_value(request)?;
        let response: SnapshotDiffResponse = self
            .http
            .request_json(Method::POST, "/snapshot/diff-full", Some(body))
            .await?;
        Ok(response.diffs)
    }

    /// Restore files from snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn restore(
        &self,
        request: &SnapshotRestoreRequest,
    ) -> Result<SnapshotRestoreResponse> {
        let body = serde_json::to_value(request)?;
        self.http
            .request_json(Method::POST, "/snapshot/restore", Some(body))
            .await
    }

    /// Revert to snapshot state.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn revert(&self, request: &SnapshotRevertRequest) -> Result<SnapshotRevertResponse> {
        let body = serde_json::to_value(request)?;
        self.http
            .request_json(Method::POST, "/snapshot/revert", Some(body))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn test_snapshot_track() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/snapshot/track"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tracked": 3
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SnapshotsApi::new(client);
        let request = SnapshotTrackRequest {
            files: vec!["src/main.rs".to_string()],
            extra: serde_json::Value::Null,
        };
        let response = api.track(&request).await.unwrap();
        assert_eq!(response.tracked, 3);
    }

    #[tokio::test]
    async fn test_snapshot_patch() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/snapshot/patch"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "patches": [
                    {"path": "src/lib.rs", "patch": "@@ -1 +1 @@", "modified": true}
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SnapshotsApi::new(client);
        let request = SnapshotPatchRequest {
            session_id: Some("sess-123".to_string()),
            ..Default::default()
        };
        let response = api.patch(&request).await.unwrap();
        assert_eq!(response.patches.len(), 1);
        assert!(response.patches[0].modified);
    }

    #[tokio::test]
    async fn test_snapshot_diff() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/snapshot/diff"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "diffs": [
                    {"path": "README.md", "status": "modified"}
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SnapshotsApi::new(client);
        let request = SnapshotDiffRequest {
            session_id: Some("sess-123".to_string()),
            ..Default::default()
        };
        let response = api.diff(&request).await.unwrap();
        assert_eq!(response.diffs.len(), 1);
    }

    #[tokio::test]
    async fn test_snapshot_diff_full() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/snapshot/diff-full"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "diffs": [
                    {
                        "path": "README.md",
                        "original": "# Old",
                        "modified": "# New",
                        "status": "modified"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SnapshotsApi::new(client);
        let request = SnapshotDiffRequest {
            session_id: Some("sess-123".to_string()),
            ..Default::default()
        };
        let diffs = api.diff_full(&request).await.unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].original, Some("# Old".to_string()));
    }

    #[tokio::test]
    async fn test_snapshot_restore() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/snapshot/restore"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "restored": 2
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SnapshotsApi::new(client);
        let request = SnapshotRestoreRequest {
            session_id: Some("sess-123".to_string()),
            ..Default::default()
        };
        let response = api.restore(&request).await.unwrap();
        assert_eq!(response.restored, 2);
    }

    #[tokio::test]
    async fn test_snapshot_revert() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/snapshot/revert"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "reverted": 5
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SnapshotsApi::new(client);
        let request = SnapshotRevertRequest {
            session_id: Some("sess-123".to_string()),
            ..Default::default()
        };
        let response = api.revert(&request).await.unwrap();
        assert!(response.success);
        assert_eq!(response.reverted, 5);
    }
}
