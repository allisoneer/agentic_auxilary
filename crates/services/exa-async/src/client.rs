use backon::{ExponentialBuilder, Retryable};
use serde::{Serialize, de::DeserializeOwned};

use crate::{config::Config, error::ExaError, retry};

/// Exa API client
///
/// The client is generic over a [`Config`] implementation that provides authentication
/// and API configuration.
#[derive(Debug, Clone)]
pub struct Client<C: Config> {
    http: reqwest::Client,
    config: C,
    backoff: ExponentialBuilder,
}

impl Client<crate::config::ExaConfig> {
    /// Creates a new client with default configuration
    ///
    /// Uses environment variables for authentication:
    /// - `EXA_API_KEY` for API key authentication
    /// - `EXA_BASE_URL` for custom API base URL
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(crate::config::ExaConfig::new())
    }
}

impl<C: Config + Default> Default for Client<C> {
    fn default() -> Self {
        Self::with_config(C::default())
    }
}

impl<C: Config> Client<C> {
    /// Creates a new client with the given configuration.
    ///
    /// # Panics
    ///
    /// Panics if the reqwest client cannot be built.
    #[must_use]
    pub fn with_config(config: C) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
            config,
            backoff: retry::default_backoff_builder(),
        }
    }

    /// Replaces the HTTP client with a custom one
    #[must_use]
    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    /// Replaces the backoff configuration for retry logic
    #[must_use]
    pub fn with_backoff(mut self, backoff: ExponentialBuilder) -> Self {
        self.backoff = backoff;
        self
    }

    /// Returns a reference to the client's configuration
    #[must_use]
    pub const fn config(&self) -> &C {
        &self.config
    }

    pub(crate) async fn post<I, O>(&self, path: &str, body: I) -> Result<O, ExaError>
    where
        I: Serialize + Send + Sync,
        O: DeserializeOwned,
    {
        let mk = || async {
            let headers = self.config.headers()?;
            Ok(self
                .http
                .post(self.config.url(path))
                .headers(headers)
                .query(&self.config.query())
                .json(&body)
                .build()?)
        };
        self.execute(mk).await
    }

    async fn execute<O, M, Fut>(&self, mk: M) -> Result<O, ExaError>
    where
        O: DeserializeOwned,
        M: Fn() -> Fut + Send + Sync,
        Fut: core::future::Future<Output = Result<reqwest::Request, ExaError>> + Send,
    {
        // Validate auth before any request
        self.config.validate_auth()?;

        let bytes = self.execute_raw(mk).await?;
        let resp: O =
            serde_json::from_slice(&bytes).map_err(|e| crate::error::map_deser(&e, &bytes))?;
        Ok(resp)
    }

    async fn execute_raw<M, Fut>(&self, mk: M) -> Result<bytes::Bytes, ExaError>
    where
        M: Fn() -> Fut + Send + Sync,
        Fut: core::future::Future<Output = Result<reqwest::Request, ExaError>> + Send,
    {
        let http_client = self.http.clone();

        (|| async {
            let request = mk().await?;
            let response = http_client
                .execute(request)
                .await
                .map_err(ExaError::Reqwest)?;

            let status = response.status();
            let bytes = response.bytes().await.map_err(ExaError::Reqwest)?;

            if status.is_success() {
                return Ok(bytes);
            }

            Err(crate::error::deserialize_api_error(status, &bytes))
        })
        .retry(self.backoff)
        .when(ExaError::is_retryable)
        .await
    }
}
