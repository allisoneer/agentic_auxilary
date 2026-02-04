use crate::{
    client::Client,
    config::Config,
    error::ExaError,
    types::contents::{ContentsRequest, ContentsResponse},
};

/// API resource for the `/contents` endpoint
pub struct Contents<'c, C: Config> {
    client: &'c Client<C>,
}

impl<'c, C: Config> Contents<'c, C> {
    /// Creates a new Contents resource
    #[must_use]
    pub const fn new(client: &'c Client<C>) -> Self {
        Self { client }
    }

    /// Retrieve content for a list of URLs
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the API returns an error.
    pub async fn create(&self, req: ContentsRequest) -> Result<ContentsResponse, ExaError> {
        self.client.post("/contents", req).await
    }
}

impl<C: Config> crate::Client<C> {
    /// Returns the Contents API resource
    #[must_use]
    pub const fn contents(&self) -> Contents<'_, C> {
        Contents::new(self)
    }
}
