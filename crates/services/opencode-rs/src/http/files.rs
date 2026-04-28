//! Files API for `OpenCode`.
//!
//! Endpoints for file operations.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::file::FileContent;
use crate::types::file::FileInfo;
use crate::types::file::FileStatus;
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
        self.http
            .request_json_with_query(
                Method::GET,
                "/file/content",
                &[("path", path.to_string())],
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
    use wiremock::matchers::query_param;

    #[tokio::test]
    async fn test_list_files_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/file"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"path": "src/main.rs", "type": "file"},
                {"path": "src/lib.rs", "type": "file"}
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let files = FilesApi::new(http);
        let result = files.list().await;
        assert!(result.is_ok());
        let file_list = result.unwrap();
        assert_eq!(file_list.len(), 2);
    }

    #[tokio::test]
    async fn test_read_file_success() {
        let mock_server = MockServer::start().await;

        // Use a simple filename without slashes to avoid URL encoding issues
        Mock::given(method("GET"))
            .and(path("/file/content"))
            .and(query_param("path", "main.rs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "path": "main.rs",
                "content": "fn main() {}"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let files = FilesApi::new(http);
        let result = files.read("main.rs").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_status_files_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/file/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"path": "src/main.rs", "status": "modified"},
                {"path": "new_file.rs", "status": "untracked"}
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let files = FilesApi::new(http);
        let result = files.status().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let mock_server = MockServer::start().await;

        // Use wiremock's any() matcher for the query param since path encoding may vary
        Mock::given(method("GET"))
            .and(path("/file/content"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "File not found"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let files = FilesApi::new(http);
        let result = files.read("nonexistent.rs").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }
}
