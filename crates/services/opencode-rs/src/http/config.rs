//! Config API for OpenCode.
//!
//! Endpoints for configuration management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::config::{Config, ConfigProviders, UpdateConfigRequest};
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
