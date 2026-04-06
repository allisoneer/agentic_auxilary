//! Find API for `OpenCode`.
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
            .request_json(Method::GET, &format!("/find?pattern={encoded}"), None)
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
            .request_json(Method::GET, &format!("/find/file?query={encoded}"), None)
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
            .request_json(Method::GET, &format!("/find/symbol?query={encoded}"), None)
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
    async fn test_find_text_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/find"))
            .and(query_param("pattern", "SEARCH_TERM"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "matches": [
                    {"file": "src/main.rs", "line": 10, "content": "// SEARCH_TERM: fix this"}
                ]
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let find = FindApi::new(http);
        let result = find.text("SEARCH_TERM").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_find_files_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/find/file"))
            .and(query_param("query", "main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "files": [
                    {"path": "src/main.rs", "score": 1.0}
                ]
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let find = FindApi::new(http);
        let result = find.files("main").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_find_symbols_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/find/symbol"))
            .and(query_param("query", "main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "symbols": []
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let find = FindApi::new(http);
        let result = find.symbols("main").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_find_text_empty_results() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/find"))
            .and(query_param("pattern", "nonexistent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "matches": []
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let find = FindApi::new(http);
        let result = find.text("nonexistent").await;
        assert!(result.is_ok());
    }
}
