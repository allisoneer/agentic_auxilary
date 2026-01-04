//! HTTP client for OpenCode REST API.
//!
//! This module provides the core HTTP client and resource API modules.

use crate::error::{OpencodeError, Result};
use reqwest::{Client as ReqClient, Method, Response};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::time::Duration;

pub mod config;
pub mod files;
pub mod find;
pub mod mcp;
pub mod messages;
pub mod misc;
pub mod parts;
pub mod permissions;
pub mod project;
pub mod providers;
pub mod pty;
pub mod sessions;
pub mod tools;
pub mod worktree;

/// Configuration for the HTTP client.
#[derive(Clone)]
pub struct HttpConfig {
    /// Base URL for the OpenCode server.
    pub base_url: String,
    /// Optional directory context header.
    pub directory: Option<String>,
    /// Request timeout.
    pub timeout: Duration,
}

/// HTTP client for OpenCode REST API.
#[derive(Clone)]
pub struct HttpClient {
    inner: ReqClient,
    cfg: HttpConfig,
}

impl HttpClient {
    /// Create a new HTTP client with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built.
    pub fn new(cfg: HttpConfig) -> Result<Self> {
        let inner = ReqClient::builder()
            .timeout(cfg.timeout)
            .build()
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Ok(Self { inner, cfg })
    }

    /// Create from base URL, directory, and optional existing client.
    pub fn from_parts(
        base_url: url::Url,
        directory: Option<PathBuf>,
        http: Option<ReqClient>,
    ) -> Self {
        Self {
            inner: http.unwrap_or_default(),
            cfg: HttpConfig {
                base_url: base_url.to_string().trim_end_matches('/').to_string(),
                directory: directory.map(|p| p.to_string_lossy().to_string()),
                timeout: Duration::from_secs(300),
            },
        }
    }

    /// Get the base URL.
    pub fn base(&self) -> &str {
        &self.cfg.base_url
    }

    /// Get the directory context.
    pub fn directory(&self) -> Option<&str> {
        self.cfg.directory.as_deref()
    }

    /// Build request headers including directory context.
    fn build_request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.cfg.base_url, path);
        let mut req = self.inner.request(method, &url);

        if let Some(dir) = &self.cfg.directory {
            req = req.header("x-opencode-directory", dir);
        }

        req
    }

    // ==================== Typed HTTP Methods ====================

    /// GET request returning deserialized JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response cannot be deserialized.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .build_request(Method::GET, path)
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::map_json_response(resp).await
    }

    /// DELETE request returning deserialized JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response cannot be deserialized.
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .build_request(Method::DELETE, path)
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::map_json_response(resp).await
    }

    /// DELETE request expecting no response body.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete_empty(&self, path: &str) -> Result<()> {
        let resp = self
            .build_request(Method::DELETE, path)
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::check_status(resp).await
    }

    /// POST request with JSON body returning deserialized JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response cannot be deserialized.
    pub async fn post<TReq: serde::Serialize, TRes: DeserializeOwned>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<TRes> {
        let resp = self
            .build_request(Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::map_json_response(resp).await
    }

    /// POST request expecting no response body.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn post_empty<TReq: serde::Serialize>(&self, path: &str, body: &TReq) -> Result<()> {
        let resp = self
            .build_request(Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::check_status(resp).await
    }

    /// PATCH request with JSON body returning deserialized JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response cannot be deserialized.
    pub async fn patch<TReq: serde::Serialize, TRes: DeserializeOwned>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<TRes> {
        let resp = self
            .build_request(Method::PATCH, path)
            .json(body)
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::map_json_response(resp).await
    }

    /// PUT request with JSON body returning deserialized JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response cannot be deserialized.
    pub async fn put<TReq: serde::Serialize, TRes: DeserializeOwned>(
        &self,
        path: &str,
        body: &TReq,
    ) -> Result<TRes> {
        let resp = self
            .build_request(Method::PUT, path)
            .json(body)
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::map_json_response(resp).await
    }

    // ==================== Legacy Methods (for backwards compatibility) ====================

    /// Make a JSON request and deserialize the response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be deserialized.
    pub async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let mut req = self.build_request(method, path);

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::map_json_response(resp).await
    }

    /// Make a request that expects no response body.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or returns a non-success status.
    pub async fn request_empty(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<()> {
        let mut req = self.build_request(method, path);

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;
        Self::check_status(resp).await
    }

    // ==================== Response Handling ====================

    /// Map response to JSON, handling errors with NamedError parsing.
    async fn map_json_response<T: DeserializeOwned>(resp: Response) -> Result<T> {
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| OpencodeError::Network(e.to_string()))?;

        if !status.is_success() {
            let body_text = String::from_utf8_lossy(&bytes);
            return Err(OpencodeError::http(status.as_u16(), &body_text));
        }

        serde_json::from_slice(&bytes).map_err(OpencodeError::from)
    }

    /// Check response status, returning error with NamedError parsing on failure.
    async fn check_status(resp: Response) -> Result<()> {
        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(OpencodeError::http(status.as_u16(), &body));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_get_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "test123",
                "value": 42
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let result: serde_json::Value = client.get("/test").await.unwrap();
        assert_eq!(result["id"], "test123");
        assert_eq!(result["value"], 42);
    }

    #[tokio::test]
    async fn test_post_with_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/create"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "new123"
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let body = serde_json::json!({"name": "test"});
        let result: serde_json::Value = client.post("/create", &body).await.unwrap();
        assert_eq!(result["id"], "new123");
    }

    #[tokio::test]
    async fn test_request_with_directory_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test"))
            .and(header("x-opencode-directory", "/my/project"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: Some("/my/project".to_string()),
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let result: serde_json::Value = client.get("/test").await.unwrap();
        assert_eq!(result, serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_error_with_named_error_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/notfound"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Session not found",
                "data": {"id": "missing123"}
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let result: Result<serde_json::Value> = client.get("/notfound").await;

        match result {
            Err(OpencodeError::Http {
                status,
                name,
                message,
                data,
            }) => {
                assert_eq!(status, 404);
                assert_eq!(name, Some("NotFound".to_string()));
                assert_eq!(message, "Session not found");
                assert!(data.is_some());
            }
            _ => panic!("Expected Http error with NamedError fields"),
        }
    }

    #[tokio::test]
    async fn test_error_with_plain_text_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/error"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let result: Result<serde_json::Value> = client.get("/error").await;

        match result {
            Err(err) => {
                assert!(err.is_server_error());
            }
            _ => panic!("Expected Http error"),
        }
    }

    #[tokio::test]
    async fn test_delete_empty() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/item/123"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        client.delete_empty("/item/123").await.unwrap();
    }

    #[tokio::test]
    async fn test_validation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/validate"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "name": "ValidationError",
                "message": "Invalid input",
                "data": {"field": "name", "reason": "required"}
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let result: Result<serde_json::Value> =
            client.post("/validate", &serde_json::json!({})).await;

        match result {
            Err(err) => {
                assert!(err.is_validation_error());
                assert_eq!(err.error_name(), Some("ValidationError"));
            }
            _ => panic!("Expected validation error"),
        }
    }
}
