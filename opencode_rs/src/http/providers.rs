//! Providers API for OpenCode.
//!
//! Endpoints for managing LLM providers.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::provider::{
    OAuthAuthorizeResponse, OAuthCallbackRequest, Provider, ProviderAuth, SetAuthRequest,
};
use reqwest::Method;

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
    /// Returns the raw provider response (structure may vary by OpenCode version).
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<serde_json::Value> {
        self.http.request_json(Method::GET, "/provider", None).await
    }

    /// List available providers as typed objects.
    ///
    /// Note: This may fail if the server returns a different structure.
    /// Use `list()` for the raw response if you encounter parsing errors.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or response cannot be parsed.
    pub async fn list_typed(&self) -> Result<Vec<Provider>> {
        self.http.request_json(Method::GET, "/provider", None).await
    }

    /// Get provider authentication info.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn auth(&self) -> Result<Vec<ProviderAuth>> {
        self.http
            .request_json(Method::GET, "/provider/auth", None)
            .await
    }

    /// Start OAuth authorization flow.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn oauth_authorize(&self, provider_id: &str) -> Result<OAuthAuthorizeResponse> {
        self.http
            .request_json(
                Method::POST,
                &format!("/provider/{}/oauth/authorize", provider_id),
                Some(serde_json::json!({})),
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
    ) -> Result<serde_json::Value> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/provider/{}/oauth/callback", provider_id),
                Some(body),
            )
            .await
    }

    /// Set authentication for a provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn set_auth(
        &self,
        provider_id: &str,
        req: &SetAuthRequest,
    ) -> Result<serde_json::Value> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::PUT, &format!("/auth/{}", provider_id), Some(body))
            .await
    }
}
