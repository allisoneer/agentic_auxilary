//! Tools, Agents, and Commands API for `OpenCode`.
//!
//! Endpoints for tool, agent, and command management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::tool::Agent;
use crate::types::tool::Command;
use crate::types::tool::Tool;
use crate::types::tool::ToolIds;
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
    async fn test_tool_ids_success() {
        let mock_server = MockServer::start().await;

        // Server sends a flat array, not {"ids": [...]}
        Mock::given(method("GET"))
            .and(path("/experimental/tool/ids"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                "read_file",
                "write_file",
                "bash"
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let tools = ToolsApi::new(http);
        let result = tools.ids().await;
        assert!(result.is_ok());
        let ids = result.unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids.0[0], "read_file");
    }

    #[tokio::test]
    async fn test_tool_list_success() {
        let mock_server = MockServer::start().await;

        // 1.3.17 ToolListItem has: id, description, parameters (no name field)
        Mock::given(method("GET"))
            .and(path("/experimental/tool"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "read_file",
                    "description": "Read contents of a file",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
                }
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let tools = ToolsApi::new(http);
        let result = tools.list().await;
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "read_file");
        assert_eq!(list[0].description, "Read contents of a file");
    }

    #[tokio::test]
    async fn test_agents_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/agent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "coder",
                    "name": "Coder Agent"
                }
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let tools = ToolsApi::new(http);
        let result = tools.agents().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_commands_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/command"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "help",
                    "name": "Help"
                }
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let tools = ToolsApi::new(http);
        let result = tools.commands().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tool_list_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/experimental/tool"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "name": "InternalError",
                "message": "Failed to list tools"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let tools = ToolsApi::new(http);
        let result = tools.list().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_server_error());
    }
}
