use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Deserialize;

/// Default Anthropic API base URL
pub const ANTHROPIC_DEFAULT_BASE: &str = "https://api.anthropic.com";
/// Default Anthropic API version
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Header name for Anthropic version
pub const HDR_ANTHROPIC_VERSION: &str = "anthropic-version";
/// Header name for Anthropic beta features
pub const HDR_ANTHROPIC_BETA: &str = "anthropic-beta";
/// Header name for API key authentication
pub const HDR_X_API_KEY: &str = "x-api-key";

/// Authentication method for Anthropic API
#[derive(Clone, PartialEq, Eq)]
pub enum AnthropicAuth {
    /// API key authentication
    ApiKey(String),
    /// Bearer token authentication
    Bearer(String),
    /// Both API key and bearer token authentication
    Both {
        /// API key for x-api-key header
        api_key: String,
        /// Bearer token for Authorization header
        bearer: String,
    },
    /// No authentication configured
    None,
}

impl std::fmt::Debug for AnthropicAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApiKey(_) => f.debug_tuple("ApiKey").field(&"<redacted>").finish(),
            Self::Bearer(_) => f.debug_tuple("Bearer").field(&"<redacted>").finish(),
            Self::Both { .. } => f
                .debug_struct("Both")
                .field("api_key", &"<redacted>")
                .field("bearer", &"<redacted>")
                .finish(),
            Self::None => f.write_str("None"),
        }
    }
}

/// Configuration for the Anthropic client
#[derive(Clone, Deserialize)]
#[serde(default)]
pub struct AnthropicConfig {
    api_base: String,
    version: String,
    #[serde(skip)]
    auth: AnthropicAuth,
    #[serde(skip)]
    beta: Vec<String>,
}

impl std::fmt::Debug for AnthropicConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicConfig")
            .field("api_base", &self.api_base)
            .field("version", &self.version)
            .field("auth", &self.auth)
            .field("beta", &self.beta)
            .finish()
    }
}

/// Helper to read and normalize an env var (trim + filter empty).
fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        let api_key = env_trimmed("ANTHROPIC_API_KEY");
        let bearer = env_trimmed("ANTHROPIC_AUTH_TOKEN");
        let api_base =
            env_trimmed("ANTHROPIC_BASE_URL").unwrap_or_else(|| ANTHROPIC_DEFAULT_BASE.into());

        let auth = match (api_key, bearer) {
            (Some(k), Some(t)) => AnthropicAuth::Both {
                api_key: k,
                bearer: t,
            },
            (Some(k), None) => AnthropicAuth::ApiKey(k),
            (None, Some(t)) => AnthropicAuth::Bearer(t),
            _ => AnthropicAuth::None,
        };

        Self {
            api_base,
            version: ANTHROPIC_VERSION.into(),
            auth,
            beta: vec![],
        }
    }
}

impl AnthropicConfig {
    /// Creates a new configuration with default settings
    ///
    /// Attempts to read from environment variables:
    /// - `ANTHROPIC_API_KEY` for API key authentication
    /// - `ANTHROPIC_AUTH_TOKEN` for bearer token authentication
    /// - `ANTHROPIC_BASE_URL` for custom API base URL (defaults to `https://api.anthropic.com`)
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the API base URL
    ///
    /// Default is `https://api.anthropic.com`
    #[must_use]
    pub fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into();
        self
    }

    /// Sets the Anthropic API version
    ///
    /// Default is `2023-06-01`
    #[must_use]
    pub fn with_version(mut self, v: impl Into<String>) -> Self {
        self.version = v.into();
        self
    }

    /// Sets API key authentication
    ///
    /// This will use the `x-api-key` header for authentication.
    #[must_use]
    pub fn with_api_key(mut self, k: impl Into<String>) -> Self {
        self.auth = AnthropicAuth::ApiKey(k.into());
        self
    }

    /// Sets bearer token authentication
    ///
    /// This will use the `Authorization: Bearer` header for authentication.
    #[must_use]
    pub fn with_bearer(mut self, t: impl Into<String>) -> Self {
        self.auth = AnthropicAuth::Bearer(t.into());
        self
    }

    /// Sets both API key and bearer token authentication
    ///
    /// This will send both the `x-api-key` and `Authorization: Bearer` headers.
    /// This matches the behavior of the official Python SDK when both credentials are present.
    #[must_use]
    pub fn with_both(mut self, api_key: impl Into<String>, bearer: impl Into<String>) -> Self {
        self.auth = AnthropicAuth::Both {
            api_key: api_key.into(),
            bearer: bearer.into(),
        };
        self
    }

    /// Sets custom beta feature strings
    ///
    /// These will be sent in the `anthropic-beta` header as a comma-separated list.
    #[must_use]
    pub fn with_beta<I, S>(mut self, beta: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.beta = beta.into_iter().map(Into::into).collect();
        self
    }

    /// Returns the configured API base URL
    #[must_use]
    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    /// Validates that authentication credentials are present and non-empty.
    ///
    /// # Errors
    ///
    /// Returns an error if neither API key nor bearer token is configured,
    /// or if the configured credentials are empty/whitespace-only.
    pub fn validate_auth(&self) -> Result<(), crate::error::AnthropicError> {
        use crate::error::AnthropicError;

        match &self.auth {
            AnthropicAuth::ApiKey(k) if !k.trim().is_empty() => Ok(()),
            AnthropicAuth::Bearer(t) if !t.trim().is_empty() => Ok(()),
            AnthropicAuth::Both { api_key, bearer }
                if !api_key.trim().is_empty() && !bearer.trim().is_empty() =>
            {
                Ok(())
            }
            _ => Err(AnthropicError::Config(
                "Missing Anthropic credentials: set ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN"
                    .into(),
            )),
        }
    }

    /// Sets beta features using the `BetaFeature` enum
    ///
    /// This is a type-safe alternative to [`with_beta`](Self::with_beta).
    #[must_use]
    pub fn with_beta_features<I: IntoIterator<Item = BetaFeature>>(mut self, features: I) -> Self {
        self.beta = features.into_iter().map(Into::<String>::into).collect();
        self
    }
}

/// Configuration trait for the Anthropic client
///
/// Implement this trait to provide custom authentication and API configuration.
pub trait Config: Send + Sync {
    /// Returns HTTP headers to include in requests
    ///
    /// # Errors
    ///
    /// Returns an error if header values contain invalid characters.
    fn headers(&self) -> Result<HeaderMap, crate::error::AnthropicError>;

    /// Constructs the full URL for an API endpoint
    fn url(&self, path: &str) -> String;

    /// Returns query parameters to include in requests
    fn query(&self) -> Vec<(&str, &str)>;

    /// Validates that authentication credentials are present.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is not properly configured.
    fn validate_auth(&self) -> Result<(), crate::error::AnthropicError>;
}

impl Config for AnthropicConfig {
    fn headers(&self) -> Result<HeaderMap, crate::error::AnthropicError> {
        use crate::error::AnthropicError;

        let mut h = HeaderMap::new();

        h.insert(
            HDR_ANTHROPIC_VERSION,
            HeaderValue::from_str(&self.version)
                .map_err(|_| AnthropicError::Config("Invalid anthropic-version header".into()))?,
        );

        if !self.beta.is_empty() {
            let v = self.beta.join(",");
            h.insert(
                HDR_ANTHROPIC_BETA,
                HeaderValue::from_str(&v)
                    .map_err(|_| AnthropicError::Config("Invalid anthropic-beta header".into()))?,
            );
        }

        match &self.auth {
            AnthropicAuth::ApiKey(k) => {
                h.insert(
                    HDR_X_API_KEY,
                    HeaderValue::from_str(k)
                        .map_err(|_| AnthropicError::Config("Invalid x-api-key value".into()))?,
                );
            }
            AnthropicAuth::Bearer(t) => {
                let v = format!("Bearer {t}");
                h.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&v).map_err(|_| {
                        AnthropicError::Config("Invalid Authorization header".into())
                    })?,
                );
            }
            AnthropicAuth::Both { api_key, bearer } => {
                h.insert(
                    HDR_X_API_KEY,
                    HeaderValue::from_str(api_key)
                        .map_err(|_| AnthropicError::Config("Invalid x-api-key value".into()))?,
                );
                let v = format!("Bearer {bearer}");
                h.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&v).map_err(|_| {
                        AnthropicError::Config("Invalid Authorization header".into())
                    })?,
                );
            }
            AnthropicAuth::None => {}
        }

        Ok(h)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    fn query(&self) -> Vec<(&str, &str)> {
        vec![]
    }

    fn validate_auth(&self) -> Result<(), crate::error::AnthropicError> {
        self.validate_auth()
    }
}

/// Known Anthropic beta features
///
/// See the [Anthropic API documentation](https://docs.anthropic.com/en/api) for details on each feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BetaFeature {
    /// Prompt caching (2024-07-31)
    PromptCaching20240731,
    /// Extended cache TTL (2025-04-11)
    ExtendedCacheTtl20250411,
    /// Token counting (2024-11-01)
    TokenCounting20241101,
    /// Structured outputs (2025-09-17) — Python SDK version
    StructuredOutputs20250917,
    /// Structured outputs (2025-11-13) — TypeScript SDK version (recommended)
    StructuredOutputs20251113,
    /// Alias to the latest structured outputs beta (currently 2025-11-13)
    StructuredOutputsLatest,
    /// Custom beta feature string
    Other(String),
}

impl From<BetaFeature> for String {
    fn from(b: BetaFeature) -> Self {
        match b {
            BetaFeature::PromptCaching20240731 => "prompt-caching-2024-07-31".into(),
            BetaFeature::ExtendedCacheTtl20250411 => "extended-cache-ttl-2025-04-11".into(),
            BetaFeature::TokenCounting20241101 => "token-counting-2024-11-01".into(),
            BetaFeature::StructuredOutputs20250917 => "structured-outputs-2025-09-17".into(),
            BetaFeature::StructuredOutputs20251113 | BetaFeature::StructuredOutputsLatest => {
                "structured-outputs-2025-11-13".into()
            }
            BetaFeature::Other(s) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_headers_exist() {
        let cfg = AnthropicConfig::new();
        let h = cfg.headers().unwrap();
        assert!(h.contains_key(super::HDR_ANTHROPIC_VERSION));
    }

    #[test]
    fn auth_api_key_header() {
        let cfg = AnthropicConfig::new().with_api_key("k123");
        let h = cfg.headers().unwrap();
        assert!(h.contains_key(HDR_X_API_KEY));
        assert!(!h.contains_key(reqwest::header::AUTHORIZATION));
    }

    #[test]
    fn auth_bearer_header() {
        let cfg = AnthropicConfig::new().with_bearer("t123");
        let h = cfg.headers().unwrap();
        assert!(h.contains_key(reqwest::header::AUTHORIZATION));
        assert!(!h.contains_key(HDR_X_API_KEY));
    }

    #[test]
    fn auth_both_headers() {
        let cfg = AnthropicConfig::new().with_both("k123", "t123");
        let h = cfg.headers().unwrap();
        assert!(h.contains_key(HDR_X_API_KEY));
        assert!(h.contains_key(reqwest::header::AUTHORIZATION));
    }

    #[test]
    fn beta_header_join() {
        let cfg = AnthropicConfig::new().with_beta(vec!["a", "b"]);
        let h = cfg.headers().unwrap();
        let v = h.get(HDR_ANTHROPIC_BETA).unwrap().to_str().unwrap();
        assert_eq!(v, "a,b");
    }

    #[test]
    fn invalid_header_values_error() {
        let cfg = AnthropicConfig::new().with_api_key("bad\nkey");
        match cfg.headers() {
            Err(crate::error::AnthropicError::Config(msg)) => assert!(msg.contains("x-api-key")),
            other => panic!("Expected Config error, got {other:?}"),
        }
    }

    #[test]
    fn validate_auth_missing() {
        let cfg = AnthropicConfig {
            api_base: "test".into(),
            version: "test".into(),
            auth: AnthropicAuth::None,
            beta: vec![],
        };
        assert!(cfg.validate_auth().is_err());
    }

    #[test]
    fn debug_output_redacts_api_key() {
        let cfg = AnthropicConfig::new().with_api_key("super-secret-key-12345");
        let debug_str = format!("{cfg:?}");

        assert!(
            !debug_str.contains("super-secret-key-12345"),
            "Debug output should not contain the API key"
        );
        assert!(
            debug_str.contains("<redacted>"),
            "Debug output should contain '<redacted>'"
        );
    }

    #[test]
    fn debug_output_redacts_bearer() {
        let cfg = AnthropicConfig::new().with_bearer("super-secret-token-12345");
        let debug_str = format!("{cfg:?}");

        assert!(
            !debug_str.contains("super-secret-token-12345"),
            "Debug output should not contain the bearer token"
        );
        assert!(
            debug_str.contains("<redacted>"),
            "Debug output should contain '<redacted>'"
        );
    }

    #[test]
    fn debug_output_redacts_both() {
        let cfg = AnthropicConfig::new().with_both("secret-api-key", "secret-bearer-token");
        let debug_str = format!("{cfg:?}");

        assert!(
            !debug_str.contains("secret-api-key"),
            "Debug output should not contain the API key"
        );
        assert!(
            !debug_str.contains("secret-bearer-token"),
            "Debug output should not contain the bearer token"
        );
        assert!(
            debug_str.contains("<redacted>"),
            "Debug output should contain '<redacted>'"
        );
    }

    #[test]
    fn validate_auth_rejects_empty_api_key() {
        let cfg = AnthropicConfig::new().with_api_key("");
        assert!(cfg.validate_auth().is_err());

        let cfg = AnthropicConfig::new().with_api_key("   ");
        assert!(cfg.validate_auth().is_err());

        let cfg = AnthropicConfig::new().with_api_key("\n");
        assert!(cfg.validate_auth().is_err());
    }

    #[test]
    fn validate_auth_rejects_empty_bearer() {
        let cfg = AnthropicConfig::new().with_bearer("");
        assert!(cfg.validate_auth().is_err());

        let cfg = AnthropicConfig::new().with_bearer("   ");
        assert!(cfg.validate_auth().is_err());
    }

    #[test]
    fn validate_auth_rejects_empty_both() {
        // Both empty
        let cfg = AnthropicConfig::new().with_both("", "");
        assert!(cfg.validate_auth().is_err());

        // API key empty, bearer valid
        let cfg = AnthropicConfig::new().with_both("", "valid-token");
        assert!(cfg.validate_auth().is_err());

        // API key valid, bearer empty
        let cfg = AnthropicConfig::new().with_both("valid-key", "");
        assert!(cfg.validate_auth().is_err());

        // Both whitespace
        let cfg = AnthropicConfig::new().with_both("   ", "   ");
        assert!(cfg.validate_auth().is_err());
    }

    #[test]
    fn validate_auth_accepts_valid_credentials() {
        let cfg = AnthropicConfig::new().with_api_key("valid-key");
        assert!(cfg.validate_auth().is_ok());

        let cfg = AnthropicConfig::new().with_bearer("valid-token");
        assert!(cfg.validate_auth().is_ok());

        let cfg = AnthropicConfig::new().with_both("valid-key", "valid-token");
        assert!(cfg.validate_auth().is_ok());

        // Valid with leading/trailing whitespace (trimmed internally)
        let cfg = AnthropicConfig::new().with_api_key("  valid-key  ");
        assert!(cfg.validate_auth().is_ok());
    }
}
