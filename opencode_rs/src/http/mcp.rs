//! MCP API for OpenCode.
//!
//! Endpoints for Model Context Protocol server management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::api::McpActionResponse;
use crate::types::mcp::{
    McpAddRequest, McpAuthCallbackRequest, McpAuthStartRequest, McpAuthStartResponse,
    McpAuthenticateRequest, McpStatus,
};
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
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, &format!("/mcp/{}/auth", name), Some(body))
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
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/mcp/{}/auth/callback", name),
                Some(body),
            )
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
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/mcp/{}/auth/authenticate", name),
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
        self.http
            .request_empty(Method::DELETE, &format!("/mcp/{}/auth", name), None)
            .await
    }

    /// Connect to an MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn connect(&self, name: &str) -> Result<McpActionResponse> {
        self.http
            .request_json(
                Method::POST,
                &format!("/mcp/{}/connect", name),
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
        self.http
            .request_json(
                Method::POST,
                &format!("/mcp/{}/disconnect", name),
                None, // OpenCode API expects no request body
            )
            .await
    }
}
