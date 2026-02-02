use reqwest::header::{HeaderMap, HeaderValue};

/// Default Exa API base URL
pub const EXA_DEFAULT_BASE: &str = "https://api.exa.ai";
/// Header name for API key authentication
pub const HDR_X_API_KEY: &str = "x-api-key";

/// Configuration for the Exa client
#[derive(Debug, Clone)]
pub struct ExaConfig {
    api_base: String,
    api_key: Option<String>,
}

impl Default for ExaConfig {
    fn default() -> Self {
        let api_key = std::env::var("EXA_API_KEY").ok();
        let api_base = std::env::var("EXA_BASE_URL").unwrap_or_else(|_| EXA_DEFAULT_BASE.into());

        Self { api_base, api_key }
    }
}

impl ExaConfig {
    /// Creates a new configuration with default settings
    ///
    /// Attempts to read from environment variables:
    /// - `EXA_API_KEY` for API key authentication
    /// - `EXA_BASE_URL` for custom API base URL (defaults to `https://api.exa.ai`)
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the API base URL
    #[must_use]
    pub fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into();
        self
    }

    /// Sets the API key
    #[must_use]
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Returns the configured API base URL
    #[must_use]
    pub fn api_base(&self) -> &str {
        &self.api_base
    }
}

/// Configuration trait for the Exa client
///
/// Implement this trait to provide custom authentication and API configuration.
pub trait Config: Send + Sync {
    /// Returns HTTP headers to include in requests
    ///
    /// # Errors
    ///
    /// Returns an error if header values contain invalid characters.
    fn headers(&self) -> Result<HeaderMap, crate::error::ExaError>;

    /// Constructs the full URL for an API endpoint
    fn url(&self, path: &str) -> String;

    /// Returns query parameters to include in requests
    fn query(&self) -> Vec<(&str, &str)>;

    /// Validates that authentication credentials are present.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is not properly configured.
    fn validate_auth(&self) -> Result<(), crate::error::ExaError>;
}

impl Config for ExaConfig {
    fn headers(&self) -> Result<HeaderMap, crate::error::ExaError> {
        use crate::error::ExaError;

        let mut h = HeaderMap::new();

        if let Some(key) = &self.api_key {
            h.insert(
                HDR_X_API_KEY,
                HeaderValue::from_str(key)
                    .map_err(|_| ExaError::Config("Invalid x-api-key value".into()))?,
            );
        }

        Ok(h)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    fn query(&self) -> Vec<(&str, &str)> {
        vec![]
    }

    fn validate_auth(&self) -> Result<(), crate::error::ExaError> {
        if self.api_key.is_none() {
            return Err(crate::error::ExaError::Config(
                "Missing Exa credentials: set EXA_API_KEY environment variable".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::EnvGuard;
    use serial_test::serial;

    #[test]
    #[serial(env)]
    fn config_reads_env_vars() {
        let _key = EnvGuard::set("EXA_API_KEY", "test-key-123");
        let _base = EnvGuard::set("EXA_BASE_URL", "https://custom.exa.ai");

        let cfg = ExaConfig::new();
        assert_eq!(cfg.api_base(), "https://custom.exa.ai");

        let h = cfg.headers().unwrap();
        assert_eq!(
            h.get(HDR_X_API_KEY).unwrap().to_str().unwrap(),
            "test-key-123"
        );
    }

    #[test]
    #[serial(env)]
    fn config_defaults_base_url() {
        let _key = EnvGuard::set("EXA_API_KEY", "k");
        let _base = EnvGuard::remove("EXA_BASE_URL");

        let cfg = ExaConfig::new();
        assert_eq!(cfg.api_base(), EXA_DEFAULT_BASE);
    }

    #[test]
    #[serial(env)]
    fn validate_auth_missing_key() {
        let _key = EnvGuard::remove("EXA_API_KEY");

        let cfg = ExaConfig::new();
        assert!(cfg.validate_auth().is_err());
    }

    #[test]
    #[serial(env)]
    fn validate_auth_with_key() {
        let _key = EnvGuard::set("EXA_API_KEY", "test-key");

        let cfg = ExaConfig::new();
        assert!(cfg.validate_auth().is_ok());
    }

    #[test]
    fn builder_methods() {
        let cfg = ExaConfig::new()
            .with_api_base("https://test.exa.ai")
            .with_api_key("my-key");

        assert_eq!(cfg.api_base(), "https://test.exa.ai");
        assert!(cfg.validate_auth().is_ok());

        let h = cfg.headers().unwrap();
        assert_eq!(h.get(HDR_X_API_KEY).unwrap().to_str().unwrap(), "my-key");
    }
}
