use crate::{
    client::Client,
    config::Config,
    error::ExaError,
    types::find_similar::{FindSimilarRequest, FindSimilarResponse},
};

/// API resource for the `/findSimilar` endpoint
pub struct FindSimilar<'c, C: Config> {
    client: &'c Client<C>,
}

impl<'c, C: Config> FindSimilar<'c, C> {
    /// Creates a new `FindSimilar` resource
    #[must_use]
    pub const fn new(client: &'c Client<C>) -> Self {
        Self { client }
    }

    /// Find pages similar to the given URL
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or the API returns an error.
    pub async fn create(&self, req: FindSimilarRequest) -> Result<FindSimilarResponse, ExaError> {
        self.client.post("/findSimilar", req).await
    }
}

impl<C: Config> crate::Client<C> {
    /// Returns the `FindSimilar` API resource
    #[must_use]
    pub const fn find_similar(&self) -> FindSimilar<'_, C> {
        FindSimilar::new(self)
    }
}
