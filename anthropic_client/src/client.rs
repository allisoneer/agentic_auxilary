use serde::{Serialize, de::DeserializeOwned};

use crate::{config::Config, error::AnthropicError, retry};

#[derive(Debug, Clone)]
pub struct Client<C: Config> {
    http: reqwest::Client,
    config: C,
    backoff: backoff::ExponentialBackoff,
}

impl Client<crate::config::AnthropicConfig> {
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

    #[must_use]
    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    #[must_use]
    pub const fn with_backoff(mut self, backoff: backoff::ExponentialBackoff) -> Self {
        self.backoff = backoff;
        self
    }

    #[must_use]
    pub const fn config(&self) -> &C {
        &self.config
    }

    pub(crate) async fn get<O: DeserializeOwned>(&self, path: &str) -> Result<O, AnthropicError> {
        let request = self
            .http
            .get(self.config.url(path))
            .headers(self.config.headers())
            .query(&self.config.query())
            .build()?;

        let response = self.http.execute(request).await?;
        let body = response.bytes().await?;
        let resp: O = serde_json::from_slice(&body).map_err(|e| {
            AnthropicError::Serde(format!("{}: {}", e, String::from_utf8_lossy(&body)))
        })?;
        Ok(resp)
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
        let request = self
            .http
            .get(self.config.url(path))
            .headers(self.config.headers())
            .query(&self.config.query())
            .query(query)
            .build()?;

        let response = self.http.execute(request).await?;
        let body = response.bytes().await?;
        let resp: O = serde_json::from_slice(&body).map_err(|e| {
            AnthropicError::Serde(format!("{}: {}", e, String::from_utf8_lossy(&body)))
        })?;
        Ok(resp)
    }
}
