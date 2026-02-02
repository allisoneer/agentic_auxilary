#![deny(warnings)]
#![deny(clippy::all)]

//! Web fetch and web search MCP tools.

pub mod fetch;
pub mod haiku;
pub mod search;
pub mod tools;
pub mod types;

use tokio::sync::OnceCell;

/// Shared state container for web tools.
///
/// Wraps shared HTTP clients and lazy-initialized Anthropic client
/// for reuse across MCP calls.
pub struct WebTools {
    /// Shared HTTP client for fetching web pages
    pub(crate) http: reqwest::Client,
    /// Exa search API client
    pub(crate) exa: exa_async::Client<exa_async::ExaConfig>,
    /// Lazy-initialized Anthropic client for Haiku summarization
    pub(crate) anthropic: OnceCell<anthropic_async::Client<anthropic_async::AnthropicConfig>>,
}

impl WebTools {
    /// Create a new `WebTools` instance with default clients.
    ///
    /// # Panics
    /// Panics if the reqwest HTTP client cannot be built.
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            exa: exa_async::Client::new(),
            anthropic: OnceCell::new(),
        }
    }
}

impl Default for WebTools {
    fn default() -> Self {
        Self::new()
    }
}

/// Re-export the `build_registry` function and `WebTools` for registry consumers.
pub use tools::build_registry;
