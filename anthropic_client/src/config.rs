use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde::Deserialize;

pub const ANTHROPIC_DEFAULT_BASE: &str = "https://api.anthropic.com";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const HDR_ANTHROPIC_VERSION: &str = "anthropic-version";
pub const HDR_ANTHROPIC_BETA: &str = "anthropic-beta";
pub const HDR_X_API_KEY: &str = "x-api-key";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnthropicAuth {
    ApiKey(String),
    Bearer(String),
    None,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AnthropicConfig {
    api_base: String,
    version: String,
    #[serde(skip)]
    auth: AnthropicAuth,
    #[serde(skip)]
    beta: Vec<String>,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let bearer = std::env::var("ANTHROPIC_AUTH_TOKEN").ok();

        let auth = match (api_key, bearer) {
            (Some(k), None) => AnthropicAuth::ApiKey(k),
            (None, Some(t)) => AnthropicAuth::Bearer(t),
            _ => AnthropicAuth::None,
        };

        Self {
            api_base: ANTHROPIC_DEFAULT_BASE.into(),
            version: ANTHROPIC_VERSION.into(),
            auth,
            beta: vec![],
        }
    }
}

impl AnthropicConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into();
        self
    }

    #[must_use]
    pub fn with_version(mut self, v: impl Into<String>) -> Self {
        self.version = v.into();
        self
    }

    #[must_use]
    pub fn with_api_key(mut self, k: impl Into<String>) -> Self {
        self.auth = AnthropicAuth::ApiKey(k.into());
        self
    }

    #[must_use]
    pub fn with_bearer(mut self, t: impl Into<String>) -> Self {
        self.auth = AnthropicAuth::Bearer(t.into());
        self
    }

    #[must_use]
    pub fn with_beta<I, S>(mut self, beta: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.beta = beta.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    /// Validates that authentication credentials are present.
    ///
    /// # Errors
    ///
    /// Returns an error if neither API key nor bearer token is configured.
    pub fn validate_auth(&self) -> Result<(), crate::error::AnthropicError> {
        match &self.auth {
            AnthropicAuth::None => Err(crate::error::AnthropicError::Config(
                "Missing Anthropic credentials: set ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN"
                    .into(),
            )),
            _ => Ok(()),
        }
    }

    #[must_use]
    pub fn with_beta_features<I: IntoIterator<Item = BetaFeature>>(mut self, features: I) -> Self {
        self.beta = features.into_iter().map(Into::<String>::into).collect();
        self
    }
}

pub trait Config: Send + Sync {
    fn headers(&self) -> HeaderMap;
    fn url(&self, path: &str) -> String;
    fn query(&self) -> Vec<(&str, &str)>;

    /// Validates that authentication credentials are present.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is not properly configured.
    fn validate_auth(&self) -> Result<(), crate::error::AnthropicError>;
}

impl Config for AnthropicConfig {
    fn headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();

        h.insert(
            HDR_ANTHROPIC_VERSION,
            HeaderValue::from_str(&self.version).unwrap(),
        );

        if !self.beta.is_empty() {
            let v = self.beta.join(",");
            h.insert(HDR_ANTHROPIC_BETA, HeaderValue::from_str(&v).unwrap());
        }

        match &self.auth {
            AnthropicAuth::ApiKey(k) => {
                h.insert(HDR_X_API_KEY, HeaderValue::from_str(k).unwrap());
            }
            AnthropicAuth::Bearer(t) => {
                let v = format!("Bearer {t}");
                h.insert(AUTHORIZATION, HeaderValue::from_str(&v).unwrap());
            }
            AnthropicAuth::None => {}
        }

        h
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BetaFeature {
    PromptCaching20240731,
    ExtendedCacheTtl20250411,
    TokenCounting20241101,
    Other(String),
}

impl From<BetaFeature> for String {
    fn from(b: BetaFeature) -> Self {
        match b {
            BetaFeature::PromptCaching20240731 => "prompt-caching-2024-07-31".into(),
            BetaFeature::ExtendedCacheTtl20250411 => "extended-cache-ttl-2025-04-11".into(),
            BetaFeature::TokenCounting20241101 => "token-counting-2024-11-01".into(),
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
        let h = cfg.headers();
        assert!(h.contains_key(super::HDR_ANTHROPIC_VERSION));
    }

    #[test]
    fn auth_api_key_header() {
        let cfg = AnthropicConfig::new().with_api_key("k123");
        let h = cfg.headers();
        assert!(h.contains_key(HDR_X_API_KEY));
        assert!(!h.contains_key(reqwest::header::AUTHORIZATION));
    }

    #[test]
    fn auth_bearer_header() {
        let cfg = AnthropicConfig::new().with_bearer("t123");
        let h = cfg.headers();
        assert!(h.contains_key(reqwest::header::AUTHORIZATION));
        assert!(!h.contains_key(HDR_X_API_KEY));
    }

    #[test]
    fn beta_header_join() {
        let cfg = AnthropicConfig::new().with_beta(vec!["a", "b"]);
        let h = cfg.headers();
        let v = h.get(HDR_ANTHROPIC_BETA).unwrap().to_str().unwrap();
        assert_eq!(v, "a,b");
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
}
