use crate::{
    client::Client,
    config::Config,
    error::AnthropicError,
    types::models::{Model, ModelListParams, ModelsListResponse},
};

/// API resource for the `/v1/models` endpoints
///
/// Provides methods to list and retrieve model information.
pub struct Models<'c, C: Config> {
    client: &'c Client<C>,
}

impl<'c, C: Config> Models<'c, C> {
    /// Creates a new Models resource
    pub const fn new(client: &'c Client<C>) -> Self {
        Self { client }
    }

    /// Lists all available models with optional pagination parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or the response cannot be parsed.
    pub async fn list(&self, params: &ModelListParams) -> Result<ModelsListResponse, AnthropicError> {
        self.client.get_with_query("/v1/models", params).await
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
    /// Returns the Models API resource
    #[must_use]
    pub const fn models(&self) -> Models<'_, C> {
        Models::new(self)
    }
}
