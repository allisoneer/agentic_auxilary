//! Miscellaneous API endpoints for `OpenCode`.
//!
//! Includes: VCS, path, instance, log, LSP, formatter, global endpoints.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::api::FormatterInfo;
use crate::types::api::LspServerStatus;
use crate::types::api::OpenApiDoc;
use reqwest::Method;
use serde::Deserialize;
use serde::Serialize;

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

    /// Get LSP server status for all configured LSP servers.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn lsp(&self) -> Result<Vec<LspServerStatus>> {
        self.http.request_json(Method::GET, "/lsp", None).await
    }

    /// Get formatter status for all configured formatters.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn formatter(&self) -> Result<Vec<FormatterInfo>> {
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
    pub async fn global_dispose(&self) -> Result<bool> {
        self.http
            .request_json::<bool>(Method::POST, "/global/dispose", Some(serde_json::json!({})))
            .await
    }

    /// Get `OpenAPI` spec.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn doc(&self) -> Result<OpenApiDoc> {
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
// TODO(3): Consider using enum for `level` field (Debug/Info/Warn/Error) with #[serde(other)] for forward-compat
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
    async fn test_health_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/global/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "healthy": true,
                "version": "1.3.13"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.health().await;
        assert!(result.is_ok());
        let health = result.unwrap();
        assert!(health.healthy);
        assert_eq!(health.version, Some("1.3.13".to_string()));
    }

    #[tokio::test]
    async fn test_doc_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/doc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "openapi": "3.0.0",
                "info": {"title": "OpenCode API", "version": "1.3.13"},
                "paths": {}
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.doc().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_path_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "directory": "/home/user/project"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.path().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_vcs_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/vcs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "git",
                "branch": "main"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.vcs().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dispose_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/instance/dispose"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.dispose().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_lsp_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/lsp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.lsp().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_formatter_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/formatter"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.formatter().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_global_dispose_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/global/dispose"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(true)))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.global_dispose().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_health_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/global/health"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "name": "InternalError",
                "message": "Health check failed"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.health().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_server_error());
    }

    #[tokio::test]
    async fn test_log_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/log"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc
            .log(&LogEntry {
                level: "info".to_string(),
                message: "Test log message".to_string(),
                data: None,
            })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_dispose_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/instance/dispose"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "name": "InternalError",
                "message": "Failed to dispose instance"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let misc = MiscApi::new(http);
        let result = misc.dispose().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_server_error());
    }
}
