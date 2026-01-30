//! Tools, Agents, and Commands API for OpenCode.
//!
//! Endpoints for tool, agent, and command management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::tool::{Agent, Command, Tool, ToolIds};
use reqwest::Method;

/// Tools API client.
#[derive(Clone)]
pub struct ToolsApi {
    http: HttpClient,
}

impl ToolsApi {
    /// Create a new Tools API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Get tool IDs (experimental).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn ids(&self) -> Result<ToolIds> {
        self.http
            .request_json(Method::GET, "/experimental/tool/ids", None)
            .await
    }

    /// List tools (experimental).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<Tool>> {
        self.http
            .request_json(Method::GET, "/experimental/tool", None)
            .await
    }

    /// List agents.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn agents(&self) -> Result<Vec<Agent>> {
        self.http.request_json(Method::GET, "/agent", None).await
    }

    /// List commands.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn commands(&self) -> Result<Vec<Command>> {
        self.http.request_json(Method::GET, "/command", None).await
    }
}
