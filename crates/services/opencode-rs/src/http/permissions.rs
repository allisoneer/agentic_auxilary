//! Permissions API for `OpenCode`.
//!
//! Endpoints for managing permission requests.

use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::permission::PermissionReplyRequest;
use crate::types::permission::PermissionRequest;
use reqwest::Method;

/// Permissions API client.
#[derive(Clone)]
pub struct PermissionsApi {
    http: HttpClient,
}

impl PermissionsApi {
    /// Create a new Permissions API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List pending permission requests.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<PermissionRequest>> {
        self.http
            .request_json(Method::GET, "/permission", None)
            .await
    }

    /// Reply to a permission request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn reply(&self, request_id: &str, reply: &PermissionReplyRequest) -> Result<bool> {
        let rid = encode_path_segment(request_id);
        let body = serde_json::to_value(reply)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/permission/{rid}/reply"),
                Some(body),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use crate::types::permission::PermissionReply;
    use std::time::Duration;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn test_list_permissions_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "perm1",
                    "sessionID": "s1",
                    "permission": "file.write",
                    "patterns": ["/home/user/file.txt"]
                }
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let permissions = PermissionsApi::new(http);
        let result = permissions.list().await;
        assert!(result.is_ok());
        let perms = result.unwrap();
        assert_eq!(perms.len(), 1);
    }

    #[tokio::test]
    async fn test_list_permissions_empty() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let permissions = PermissionsApi::new(http);
        let result = permissions.list().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_reply_permission_approve() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/permission/perm1/reply"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(true)))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let permissions = PermissionsApi::new(http);
        let result = permissions
            .reply(
                "perm1",
                &PermissionReplyRequest {
                    reply: PermissionReply::Always,
                    message: None,
                },
            )
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_reply_permission_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/permission/missing/reply"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Permission request not found"
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

        let permissions = PermissionsApi::new(http);
        let result = permissions
            .reply(
                "missing",
                &PermissionReplyRequest {
                    reply: PermissionReply::Once,
                    message: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }
}
