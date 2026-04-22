//! Providers API for `OpenCode`.
//!
//! Endpoints for managing LLM providers.

use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::provider::OAuthAuthorizeRequest;
use crate::types::provider::OAuthAuthorizeResponse;
use crate::types::provider::OAuthCallbackRequest;
use crate::types::provider::ProviderAuthMethod;
use crate::types::provider::ProviderListResponse;
use crate::types::provider::SetAuthRequest;
use reqwest::Method;
use std::collections::HashMap;

/// Providers API client.
#[derive(Clone)]
pub struct ProvidersApi {
    http: HttpClient,
}

impl ProvidersApi {
    /// Create a new Providers API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List available providers.
    ///
    /// Returns a response containing all providers, their default models,
    /// and which providers are connected/authenticated.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<ProviderListResponse> {
        self.http.request_json(Method::GET, "/provider", None).await
    }

    /// Get provider authentication info.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn auth(&self) -> Result<HashMap<String, Vec<ProviderAuthMethod>>> {
        self.http
            .request_json(Method::GET, "/provider/auth", None)
            .await
    }

    /// Start OAuth authorization flow.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn oauth_authorize(
        &self,
        provider_id: &str,
        req: &OAuthAuthorizeRequest,
    ) -> Result<Option<OAuthAuthorizeResponse>> {
        let pid = encode_path_segment(provider_id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/provider/{pid}/oauth/authorize"),
                Some(body),
            )
            .await
    }

    /// Complete OAuth callback.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn oauth_callback(
        &self,
        provider_id: &str,
        req: &OAuthCallbackRequest,
    ) -> Result<bool> {
        let pid = encode_path_segment(provider_id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json::<bool>(
                Method::POST,
                &format!("/provider/{pid}/oauth/callback"),
                Some(body),
            )
            .await
    }

    /// Set authentication for a provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn set_auth(&self, provider_id: &str, req: &SetAuthRequest) -> Result<bool> {
        let pid = encode_path_segment(provider_id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json::<bool>(Method::PUT, &format!("/auth/{pid}"), Some(body))
            .await
    }

    /// Delete authentication for a provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete_auth(&self, provider_id: &str) -> Result<bool> {
        let pid = encode_path_segment(provider_id);
        self.http
            .request_json::<bool>(Method::DELETE, &format!("/auth/{pid}"), None)
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
    async fn test_list_providers_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/provider"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "all": [
                    {
                        "id": "anthropic",
                        "name": "Anthropic",
                        "env": [],
                        "options": {},
                        "models": {}
                    },
                    {
                        "id": "openai",
                        "name": "OpenAI",
                        "env": [],
                        "options": {},
                        "models": {}
                    }
                ],
                "default": {},
                "connected": []
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

        let providers = ProvidersApi::new(http);
        let result = providers.list().await;
        assert!(result.is_ok());
        let provider_list = result.unwrap();
        assert_eq!(provider_list.all.len(), 2);
    }

    #[tokio::test]
    async fn test_auth_providers_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/provider/auth"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "anthropic": [
                    {"type": "api-key"}
                ]
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

        let providers = ProvidersApi::new(http);
        let result = providers.auth().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_oauth_authorize_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/provider/openai/oauth/authorize"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "url": "https://oauth.openai.com/authorize?client_id=...",
                "method": "oauth"
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

        let providers = ProvidersApi::new(http);
        let result = providers
            .oauth_authorize(
                "openai",
                &OAuthAuthorizeRequest {
                    method: "oauth".to_string(),
                    inputs: None,
                },
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_oauth_callback_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/provider/openai/oauth/callback"))
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

        let providers = ProvidersApi::new(http);
        let result = providers
            .oauth_callback(
                "openai",
                &OAuthCallbackRequest {
                    method: "oauth".to_string(),
                    code: "auth_code_123".to_string(),
                },
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_auth_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/auth/anthropic"))
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

        let providers = ProvidersApi::new(http);
        let result = providers
            .set_auth(
                "anthropic",
                &SetAuthRequest {
                    key: "sk-ant-api-key".to_string(),
                },
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_auth_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/auth/anthropic"))
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

        let providers = ProvidersApi::new(http);
        let result = providers.delete_auth("anthropic").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_oauth_authorize_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/provider/unknown/oauth/authorize"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Provider not found"
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

        let providers = ProvidersApi::new(http);
        let result = providers
            .oauth_authorize(
                "unknown",
                &OAuthAuthorizeRequest {
                    method: "oauth".to_string(),
                    inputs: None,
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }
}
