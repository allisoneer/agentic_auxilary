//! Resource API for `OpenCode`.

use crate::error::Result;
use crate::http::HttpClient;
use reqwest::Method;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

/// Resource API client.
#[derive(Clone)]
pub struct ResourceApi {
    http: HttpClient,
}

/// MCP resource information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    pub name: String,
    pub uri: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub client: String,
}

impl ResourceApi {
    /// Create a new Resource API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List all resources by URI.
    pub async fn list(&self) -> Result<HashMap<String, McpResource>> {
        self.http
            .request_json(Method::GET, "/experimental/resource", None)
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
    async fn test_list_resources() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/experimental/resource"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "file:///path/to/file.txt": {
                    "name": "file.txt",
                    "uri": "file:///path/to/file.txt",
                    "description": "Text file",
                    "mimeType": "text/plain",
                    "client": "filesystem"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = ResourceApi::new(client);
        let resources = api.list().await.unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources["file:///path/to/file.txt"].client, "filesystem");
    }
}
