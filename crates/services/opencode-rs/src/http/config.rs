//! Config API for `OpenCode`.
//!
//! Endpoints for configuration management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::config::Config;
use crate::types::config::ConfigProviders;
use crate::types::config::UpdateConfigRequest;
use reqwest::Method;

/// Config API client.
#[derive(Clone)]
pub struct ConfigApi {
    http: HttpClient,
}

impl ConfigApi {
    /// Create a new Config API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Get current configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn get(&self) -> Result<Config> {
        self.http.request_json(Method::GET, "/config", None).await
    }

    /// Update configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn update(&self, req: &UpdateConfigRequest) -> Result<Config> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::PATCH, "/config", Some(body))
            .await
    }

    /// Get provider configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn providers(&self) -> Result<ConfigProviders> {
        self.http
            .request_json(Method::GET, "/config/providers", None)
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
    async fn test_get_config_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "provider": "anthropic",
                "model": "claude-sonnet-4-20250514"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let config = ConfigApi::new(http);
        let result = config.get().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_config_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/config"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "provider": "openai",
                "model": "gpt-4"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let config = ConfigApi::new(http);
        let result = config
            .update(&UpdateConfigRequest {
                provider: Some("openai".to_string()),
                model: Some("gpt-4".to_string()),
                ..Default::default()
            })
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_providers_config_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/config/providers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "providers": [
                    {"id": "anthropic", "name": "Anthropic"},
                    {"id": "openai", "name": "OpenAI"}
                ]
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let config = ConfigApi::new(http);
        let result = config.providers().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_config_validation_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/config"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "name": "ValidationError",
                "message": "Invalid provider"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let config = ConfigApi::new(http);
        let result = config
            .update(&UpdateConfigRequest {
                provider: Some("invalid".to_string()),
                ..Default::default()
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_validation_error());
    }
}
