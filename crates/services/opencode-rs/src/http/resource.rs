//! Resource API for OpenCode.
//!
//! Experimental endpoint for resource access.

use crate::error::Result;
use crate::http::{HttpClient, encode_path_segment};
use reqwest::Method;
use serde::{Deserialize, Serialize};

/// Resource API client.
#[derive(Clone)]
pub struct ResourceApi {
    http: HttpClient,
}

/// Resource information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    /// Resource URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,

    /// Resource name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Resource content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Resource MIME type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl ResourceApi {
    /// Create a new Resource API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Get a resource by URI.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get(&self, uri: &str) -> Result<ResourceInfo> {
        let encoded_uri = encode_path_segment(uri);
        self.http
            .request_json(
                Method::GET,
                &format!("/experimental/resource?uri={}", encoded_uri),
                None,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_get_resource() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/experimental/resource"))
            .and(query_param("uri", "file:///path/to/file.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "uri": "file:///path/to/file.txt",
                "name": "file.txt",
                "content": "Hello, world!",
                "mimeType": "text/plain"
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = ResourceApi::new(client);
        let resource = api.get("file:///path/to/file.txt").await.unwrap();
        assert_eq!(resource.uri, Some("file:///path/to/file.txt".to_string()));
        assert_eq!(resource.name, Some("file.txt".to_string()));
        assert_eq!(resource.content, Some("Hello, world!".to_string()));
        assert_eq!(resource.mime_type, Some("text/plain".to_string()));
    }

    #[test]
    fn test_resource_info_minimal() {
        let json = r#"{}"#;
        let resource: ResourceInfo = serde_json::from_str(json).unwrap();
        assert!(resource.uri.is_none());
        assert!(resource.content.is_none());
    }

    #[test]
    fn test_resource_info_extra_fields() {
        let json = r#"{"uri": "test://uri", "futureField": "value"}"#;
        let resource: ResourceInfo = serde_json::from_str(json).unwrap();
        assert_eq!(resource.uri, Some("test://uri".to_string()));
        assert_eq!(resource.extra["futureField"], "value");
    }
}
