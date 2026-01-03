//! HTTP client for OpenCode REST API.
//!
//! This module provides the core HTTP client and resource API modules.

use crate::error::{OpencodeError, Result};
use reqwest::{Client as ReqClient, Method, Response};
use std::time::Duration;

pub mod messages;
pub mod sessions;

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
        let inner = ReqClient::builder().timeout(cfg.timeout).build()?;
        Ok(Self { inner, cfg })
    }

    /// Get the base URL.
    pub fn base(&self) -> &str {
        &self.cfg.base_url
    }

    /// Get the directory context.
    pub fn directory(&self) -> Option<&str> {
        self.cfg.directory.as_deref()
    }

    /// Make a JSON request and deserialize the response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the response cannot be deserialized.
    pub async fn request_json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let url = format!("{}{}", self.cfg.base_url, path);
        let mut req = self.inner.request(method, &url);

        if let Some(dir) = &self.cfg.directory {
            req = req.header("x-opencode-directory", dir);
        }

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req.send().await?;
        Self::map_json(resp).await
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
        let url = format!("{}{}", self.cfg.base_url, path);
        let mut req = self.inner.request(method, &url);

        if let Some(dir) = &self.cfg.directory {
            req = req.header("x-opencode-directory", dir);
        }

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req.send().await?;
        Self::check_status(resp).await
    }

    async fn map_json<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T> {
        let status = resp.status();
        let bytes = resp.bytes().await?;

        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes).into_owned();
            return Err(OpencodeError::UnexpectedStatus {
                status: status.as_u16(),
                body,
            });
        }

        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn check_status(resp: Response) -> Result<()> {
        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(OpencodeError::UnexpectedStatus {
                status: status.as_u16(),
                body,
            });
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
    async fn test_request_json_success() {
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

        let result: serde_json::Value = client
            .request_json(Method::GET, "/test", None)
            .await
            .unwrap();
        assert_eq!(result["id"], "test123");
        assert_eq!(result["value"], 42);
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

        let result: serde_json::Value = client
            .request_json(Method::GET, "/test", None)
            .await
            .unwrap();
        assert_eq!(result, serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_request_error_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/notfound"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let result: Result<serde_json::Value> =
            client.request_json(Method::GET, "/notfound", None).await;

        match result {
            Err(OpencodeError::UnexpectedStatus { status, body }) => {
                assert_eq!(status, 404);
                assert_eq!(body, "Not Found");
            }
            _ => panic!("Expected UnexpectedStatus error"),
        }
    }
}
