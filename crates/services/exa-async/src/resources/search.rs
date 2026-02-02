use crate::{
    client::Client,
    config::Config,
    error::ExaError,
    types::search::{SearchRequest, SearchResponse},
};

/// API resource for the `/search` endpoint
pub struct Search<'c, C: Config> {
    client: &'c Client<C>,
}

impl<'c, C: Config> Search<'c, C> {
    /// Creates a new Search resource
    #[must_use]
    pub const fn new(client: &'c Client<C>) -> Self {
        Self { client }
    }

    /// Execute a search query
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the API returns an error.
    pub async fn create(&self, req: SearchRequest) -> Result<SearchResponse, ExaError> {
        self.client.post("/search", req).await
    }
}

// Add accessor to client
impl<C: Config> crate::Client<C> {
    /// Returns the Search API resource
    #[must_use]
    pub const fn search(&self) -> Search<'_, C> {
        Search::new(self)
    }
}
