use crate::{
    client::Client,
    config::Config,
    error::ExaError,
    types::answer::{AnswerRequest, AnswerResponse},
};

/// API resource for the `/answer` endpoint (non-streaming)
pub struct Answer<'c, C: Config> {
    client: &'c Client<C>,
}

impl<'c, C: Config> Answer<'c, C> {
    /// Creates a new Answer resource
    #[must_use]
    pub const fn new(client: &'c Client<C>) -> Self {
        Self { client }
    }

    /// Generate an answer to a query using Exa's search-augmented generation
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the API returns an error.
    pub async fn create(&self, req: AnswerRequest) -> Result<AnswerResponse, ExaError> {
        self.client.post("/answer", req).await
    }
}

impl<C: Config> crate::Client<C> {
    /// Returns the Answer API resource
    #[must_use]
    pub const fn answer(&self) -> Answer<'_, C> {
        Answer::new(self)
    }
}
