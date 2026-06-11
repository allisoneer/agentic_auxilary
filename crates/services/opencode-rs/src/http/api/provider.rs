use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::v2::envelope::LocationEnvelope;
use crate::types::v2::provider::ProviderInfo;

#[derive(Clone)]
pub struct ProviderApi {
    http: HttpClient,
}

impl ProviderApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(&self) -> Result<LocationEnvelope<Vec<ProviderInfo>>> {
        self.http.api_get("/api/provider").await
    }

    pub async fn get(&self, provider_id: &str) -> Result<LocationEnvelope<ProviderInfo>> {
        let provider_id = encode_path_segment(provider_id);
        self.http
            .api_get(&format!("/api/provider/{provider_id}"))
            .await
    }
}
