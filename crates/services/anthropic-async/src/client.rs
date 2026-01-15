use serde::{Serialize, de::DeserializeOwned};

use crate::{config::Config, error::AnthropicError, retry};

/// Anthropic API client
///
/// The client is generic over a [`Config`] implementation that provides authentication
/// and API configuration.
#[derive(Debug, Clone)]
pub struct Client<C: Config> {
    http: reqwest::Client,
    config: C,
    backoff: backoff::ExponentialBackoff,
}

impl Client<crate::config::AnthropicConfig> {
    /// Creates a new client with default configuration
    ///
    /// Uses environment variables for authentication:
    /// - `ANTHROPIC_API_KEY` for API key authentication
    /// - `ANTHROPIC_AUTH_TOKEN` for bearer token authentication
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(crate::config::AnthropicConfig::new())
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
                .timeout(std::time::Duration::from_secs(600))
                .build()
                .expect("reqwest client"),
            config,
            backoff: retry::default_backoff(),
        }
    }

    /// Replaces the HTTP client with a custom one
    ///
    /// Useful for setting custom timeouts, proxies, or other HTTP configuration.
    #[must_use]
    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    /// Replaces the backoff configuration for retry logic
    ///
    /// By default, the client uses exponential backoff with jitter.
    #[must_use]
    pub const fn with_backoff(mut self, backoff: backoff::ExponentialBackoff) -> Self {
        self.backoff = backoff;
        self
    }

    /// Returns a reference to the client's configuration
    #[must_use]
    pub const fn config(&self) -> &C {
        &self.config
    }

    pub(crate) async fn get<O: DeserializeOwned>(&self, path: &str) -> Result<O, AnthropicError> {
        let mk = || async {
            let headers = self.config.headers()?;
            Ok(self
                .http
                .get(self.config.url(path))
                .headers(headers)
                .query(&self.config.query())
                .build()?)
        };
        self.execute(mk).await
    }

    pub(crate) async fn get_with_query<Q, O>(
        &self,
        path: &str,
        query: &Q,
    ) -> Result<O, AnthropicError>
    where
        Q: Serialize + Sync + ?Sized,
        O: DeserializeOwned,
    {
        let mk = || async {
            let headers = self.config.headers()?;
            Ok(self
                .http
                .get(self.config.url(path))
                .headers(headers)
                .query(&self.config.query())
                .query(query)
                .build()?)
        };
        self.execute(mk).await
    }

    pub(crate) async fn post<I, O>(&self, path: &str, body: I) -> Result<O, AnthropicError>
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

    /// Sends a POST request and returns the raw response for streaming.
    ///
    /// This method does not retry on error, as streaming responses cannot be replayed.
    #[cfg(feature = "streaming")]
    pub(crate) async fn post_stream<I: Serialize + Send + Sync>(
        &self,
        path: &str,
        body: I,
    ) -> Result<reqwest::Response, AnthropicError> {
        // Validate auth before any request
        self.config.validate_auth()?;

        let headers = self.config.headers()?;
        let request = self
            .http
            .post(self.config.url(path))
            .headers(headers)
            .query(&self.config.query())
            .json(&body)
            .build()?;

        let response = self
            .http
            .execute(request)
            .await
            .map_err(AnthropicError::Reqwest)?;

        let status = response.status();
        if status.is_success() {
            Ok(response)
        } else {
            let bytes = response.bytes().await.map_err(AnthropicError::Reqwest)?;
            Err(crate::error::deserialize_api_error(status, &bytes))
        }
    }

    async fn execute<O, M, Fut>(&self, mk: M) -> Result<O, AnthropicError>
    where
        O: DeserializeOwned,
        M: Fn() -> Fut + Send + Sync,
        Fut: core::future::Future<Output = Result<reqwest::Request, AnthropicError>> + Send,
    {
        // Validate auth before any request
        self.config.validate_auth()?;

        let bytes = self.execute_raw(mk).await?;
        let resp: O =
            serde_json::from_slice(&bytes).map_err(|e| crate::error::map_deser(&e, &bytes))?;
        Ok(resp)
    }

    async fn execute_raw<M, Fut>(&self, mk: M) -> Result<bytes::Bytes, AnthropicError>
    where
        M: Fn() -> Fut + Send + Sync,
        Fut: core::future::Future<Output = Result<reqwest::Request, AnthropicError>> + Send,
    {
        let http_client = self.http.clone();

        backoff::future::retry(self.backoff.clone(), || async {
            let request = mk().await.map_err(backoff::Error::Permanent)?;
            let response = http_client
                .execute(request)
                .await
                .map_err(AnthropicError::Reqwest)
                .map_err(backoff::Error::Permanent)?;

            let status = response.status();
            let headers = response.headers().clone();
            let bytes = response
                .bytes()
                .await
                .map_err(AnthropicError::Reqwest)
                .map_err(backoff::Error::Permanent)?;

            if status.is_success() {
                return Ok(bytes);
            }

            if crate::retry::is_retryable_status(status.as_u16()) {
                let err = crate::error::deserialize_api_error(status, &bytes);
                if let Some(retry_after) = crate::retry::parse_retry_after(&headers) {
                    return Err(backoff::Error::retry_after(err, retry_after));
                }
                return Err(backoff::Error::transient(err));
            }

            Err(backoff::Error::Permanent(
                crate::error::deserialize_api_error(status, &bytes),
            ))
        })
        .await
    }
}
