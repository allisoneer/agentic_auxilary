//! Token provider trait and authentication modes

use crate::error::AuthError;
use reqwest::header::HeaderMap;

/// Trait for providing authentication tokens/headers
pub trait TokenProvider: Send + Sync {
    /// Get headers for authenticated requests
    fn get_headers(&self) -> Result<HeaderMap, AuthError>;
    /// Check if credentials are available
    fn has_credentials(&self) -> bool;
}

/// Authentication mode enum
pub enum AuthMode<S: crate::store::SecureStore> {
    /// OAuth PKCE authentication
    OAuth(crate::oauth::OAuthPkceManager<S>),
    /// API key authentication
    ApiKey(crate::api_key::ApiKeyManager<S>),
    /// Both authentication methods
    Both {
        /// API key manager
        api_key: crate::api_key::ApiKeyManager<S>,
        /// OAuth manager
        oauth: crate::oauth::OAuthPkceManager<S>,
    },
}

impl<S: crate::store::SecureStore> AuthMode<S> {
    /// Get combined headers from all available auth methods
    pub fn get_headers(&self) -> Result<HeaderMap, AuthError> {
        let mut headers = HeaderMap::new();
        match self {
            Self::OAuth(oauth) => headers.extend(oauth.get_headers()?),
            Self::ApiKey(api_key) => headers.extend(api_key.get_headers()?),
            Self::Both { api_key, oauth } => {
                headers.extend(api_key.get_headers()?);
                headers.extend(oauth.get_headers()?);
            }
        }
        Ok(headers)
    }
}
