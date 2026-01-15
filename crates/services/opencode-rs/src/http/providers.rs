//! Providers API for OpenCode.
//!
//! Endpoints for managing LLM providers.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::api::{OAuthCallbackResponse, SetAuthResponse};
use crate::types::provider::{
    OAuthAuthorizeResponse, OAuthCallbackRequest, ProviderAuth, ProviderListResponse,
    SetAuthRequest,
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
    ) -> Result<OAuthCallbackResponse> {
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
    ) -> Result<SetAuthResponse> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::PUT, &format!("/auth/{}", provider_id), Some(body))
            .await
    }
}
