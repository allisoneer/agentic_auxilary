//! API key authentication manager

use crate::{error::AuthError, provider::TokenProvider, store::SecureStore};
use reqwest::header::{HeaderMap, HeaderValue};

const API_KEY_NAME: &str = "anthropic_api_key";

/// Manager for API key authentication
pub struct ApiKeyManager<S: SecureStore> {
    store: S,
}

impl<S: SecureStore> ApiKeyManager<S> {
    /// Create a new API key manager with the given store
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Store an API key
    pub fn set_api_key(&self, key: &str) -> Result<(), AuthError> {
        self.store.set_secret(API_KEY_NAME, key.as_bytes())
    }

    /// Retrieve the stored API key
    pub fn get_api_key(&self) -> Result<Option<String>, AuthError> {
        self.store
            .get_secret(API_KEY_NAME)?
            .map(|bytes| String::from_utf8(bytes).map_err(|e| AuthError::Other(e.to_string())))
            .transpose()
    }

    /// Clear the stored API key
    pub fn clear_api_key(&self) -> Result<(), AuthError> {
        self.store.delete_secret(API_KEY_NAME)
    }
}

impl<S: SecureStore> TokenProvider for ApiKeyManager<S> {
    fn get_headers(&self) -> Result<HeaderMap, AuthError> {
        let mut headers = HeaderMap::new();
        if let Some(key) = self.get_api_key()? {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(&key).map_err(|e| AuthError::Config(e.to_string()))?,
            );
        }
        Ok(headers)
    }

    fn has_credentials(&self) -> bool {
        self.get_api_key().ok().flatten().is_some()
    }
}
