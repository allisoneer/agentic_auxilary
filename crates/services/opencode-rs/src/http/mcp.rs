//! MCP API for `OpenCode`.
//!
//! Endpoints for Model Context Protocol server management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::api::McpActionResponse;
use crate::types::mcp::McpAddRequest;
use crate::types::mcp::McpAuthCallbackRequest;
use crate::types::mcp::McpAuthStartRequest;
use crate::types::mcp::McpAuthStartResponse;
use crate::types::mcp::McpAuthenticateRequest;
use crate::types::mcp::McpStatus;
use reqwest::Method;

/// MCP API client.
#[derive(Clone)]
pub struct McpApi {
    http: HttpClient,
}

impl McpApi {
    /// Create a new MCP API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Get MCP status.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn status(&self) -> Result<McpStatus> {
        self.http.request_json(Method::GET, "/mcp", None).await
    }

    /// Add an MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn add(&self, req: &McpAddRequest) -> Result<McpActionResponse> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/mcp", Some(body))
            .await
    }

    /// Start MCP auth flow.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn auth_start(
        &self,
        name: &str,
        req: &McpAuthStartRequest,
    ) -> Result<McpAuthStartResponse> {
        let n = encode_path_segment(name);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, &format!("/mcp/{n}/auth"), Some(body))
            .await
    }

    /// Complete MCP auth callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn auth_callback(
        &self,
        name: &str,
        req: &McpAuthCallbackRequest,
    ) -> Result<McpActionResponse> {
        let n = encode_path_segment(name);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, &format!("/mcp/{n}/auth/callback"), Some(body))
            .await
    }

    /// Authenticate with API key.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn authenticate(
        &self,
        name: &str,
        req: &McpAuthenticateRequest,
    ) -> Result<McpActionResponse> {
        let n = encode_path_segment(name);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/mcp/{n}/auth/authenticate"),
                Some(body),
            )
            .await
    }

    /// Remove MCP auth.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn auth_remove(&self, name: &str) -> Result<()> {
        let n = encode_path_segment(name);
        self.http
            .request_empty(Method::DELETE, &format!("/mcp/{n}/auth"), None)
            .await
    }

    /// Connect to an MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn connect(&self, name: &str) -> Result<McpActionResponse> {
        let n = encode_path_segment(name);
        self.http
            .request_json(
                Method::POST,
                &format!("/mcp/{n}/connect"),
                None, // OpenCode API expects no request body
            )
            .await
    }

    /// Disconnect from an MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn disconnect(&self, name: &str) -> Result<McpActionResponse> {
        let n = encode_path_segment(name);
        self.http
            .request_json(
                Method::POST,
                &format!("/mcp/{n}/disconnect"),
                None, // OpenCode API expects no request body
            )
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
    async fn test_mcp_status_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/mcp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "servers": []
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

        let mcp = McpApi::new(http);
        let result = mcp.status().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_add_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
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

        let mcp = McpApi::new(http);
        let result = mcp
            .add(&McpAddRequest {
                name: "test-server".to_string(),
                command: "npx".to_string(),
                args: vec!["@modelcontextprotocol/server-test".to_string()],
                env: None,
            })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_auth_start_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp/server1/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "url": "https://auth.example.com/oauth"
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

        let mcp = McpApi::new(http);
        let result = mcp
            .auth_start(
                "server1",
                &McpAuthStartRequest {
                    callback_url: Some("http://localhost:8080/callback".to_string()),
                },
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_auth_callback_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp/server1/auth/callback"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
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

        let mcp = McpApi::new(http);
        let result = mcp
            .auth_callback(
                "server1",
                &McpAuthCallbackRequest {
                    code: "code123".to_string(),
                    state: Some("state456".to_string()),
                },
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_connect_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp/server1/connect"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
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

        let mcp = McpApi::new(http);
        let result = mcp.connect("server1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_disconnect_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp/server1/disconnect"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
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

        let mcp = McpApi::new(http);
        let result = mcp.disconnect("server1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_auth_remove_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/mcp/server1/auth"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let mcp = McpApi::new(http);
        let result = mcp.auth_remove("server1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mcp_connect_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp/missing/connect"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "MCP server not found"
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

        let mcp = McpApi::new(http);
        let result = mcp.connect("missing").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }

    #[tokio::test]
    async fn test_mcp_authenticate_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp/server1/auth/authenticate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true
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

        let mcp = McpApi::new(http);
        let result = mcp
            .authenticate(
                "server1",
                &McpAuthenticateRequest {
                    token: "test-api-key".to_string(),
                },
            )
            .await;
        assert!(result.is_ok());
    }
}
