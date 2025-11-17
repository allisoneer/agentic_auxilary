use serde::Serialize;

use crate::{
    client::Client,
    config::Config,
    error::AnthropicError,
    types::models::{Model, ModelsListResponse},
};

pub struct Models<'c, C: Config> {
    client: &'c Client<C>,
}

impl<'c, C: Config> Models<'c, C> {
    pub const fn new(client: &'c Client<C>) -> Self {
        Self { client }
    }

    /// Lists all available models.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or the response cannot be parsed.
    pub async fn list<Q>(&self, query: &Q) -> Result<ModelsListResponse, AnthropicError>
    where
        Q: Serialize + Sync + ?Sized,
    {
        self.client.get_with_query("/v1/models", query).await
    }

    /// Gets details for a specific model.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or the response cannot be parsed.
    pub async fn get(&self, model_id: &str) -> Result<Model, AnthropicError> {
        self.client.get(&format!("/v1/models/{model_id}")).await
    }
}

impl<C: Config> crate::Client<C> {
    #[must_use]
    pub const fn models(&self) -> Models<'_, C> {
        Models::new(self)
    }
}
