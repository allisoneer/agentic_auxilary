#![deny(warnings)]
#![deny(clippy::all)]

//! Web fetch and web search MCP tools.

pub mod fetch;
pub mod haiku;
pub mod search;
pub mod tools;
pub mod types;

use agentic_config::types::{AnthropicServiceConfig, ExaServiceConfig, WebRetrievalConfig};
use tokio::sync::OnceCell;

/// Shared state container for web tools.
///
/// Wraps shared HTTP clients, lazy-initialized Anthropic client,
/// and configuration for reuse across MCP calls.
pub struct WebTools {
    /// Shared HTTP client for fetching web pages
    pub(crate) http: reqwest::Client,
    /// Exa search API client
    pub(crate) exa: exa_async::Client<exa_async::ExaConfig>,
    /// Lazy-initialized Anthropic client for Haiku summarization
    pub(crate) anthropic: OnceCell<anthropic_async::Client<anthropic_async::AnthropicConfig>>,
    /// Web retrieval configuration (timeouts, limits, summarizer settings)
    pub(crate) cfg: WebRetrievalConfig,
    /// Exa service configuration
    #[allow(dead_code)] // Will be used when Exa client supports custom base URLs
    pub(crate) exa_cfg: ExaServiceConfig,
    /// Anthropic service configuration
    #[allow(dead_code)] // Will be used when Anthropic client supports custom config
    pub(crate) anthropic_cfg: AnthropicServiceConfig,
}

impl WebTools {
    /// Create a new `WebTools` instance with custom configuration.
    ///
    /// # Panics
    /// Panics if the reqwest HTTP client cannot be built.
    #[must_use]
    pub fn with_config(
        cfg: WebRetrievalConfig,
        exa_cfg: ExaServiceConfig,
        anthropic_cfg: AnthropicServiceConfig,
    ) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(cfg.request_timeout_secs))
                .build()
                .expect("reqwest client"),
            exa: exa_async::Client::new(),
            anthropic: OnceCell::new(),
            cfg,
            exa_cfg,
            anthropic_cfg,
        }
    }

    /// Create a new `WebTools` instance with default configuration.
    ///
    /// # Panics
    /// Panics if the reqwest HTTP client cannot be built.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(
            WebRetrievalConfig::default(),
            ExaServiceConfig::default(),
            AnthropicServiceConfig::default(),
        )
    }
}

impl Default for WebTools {
    fn default() -> Self {
        Self::new()
    }
}

/// Re-export the `build_registry` function and `WebTools` for registry consumers.
pub use tools::build_registry;

#[cfg(test)]
impl WebTools {
    /// Create a `WebTools` instance with a custom HTTP client for testing.
    pub(crate) fn with_http_client(http: reqwest::Client) -> Self {
        Self {
            http,
            exa: exa_async::Client::new(),
            anthropic: OnceCell::new(),
            cfg: WebRetrievalConfig::default(),
            exa_cfg: ExaServiceConfig::default(),
            anthropic_cfg: AnthropicServiceConfig::default(),
        }
    }
}
