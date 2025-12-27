use anyhow::{Result, anyhow};
use cynic::http::ReqwestExt;
use reqwest::Client;

pub struct LinearClient {
    client: Client,
    url: String,
    api_key: String,
}

impl LinearClient {
    pub fn new(api_key: Option<String>) -> Result<Self> {
        let api_key = match api_key.or_else(|| std::env::var("LINEAR_API_KEY").ok()) {
            Some(k) if !k.is_empty() => k,
            _ => return Err(anyhow!("LINEAR_API_KEY environment variable is not set")),
        };

        let url = std::env::var("LINEAR_GRAPHQL_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "https://api.linear.app/graphql".to_string());

        let client = Client::builder().user_agent("linear-tools/0.1.0").build()?;

        Ok(Self {
            client,
            url,
            api_key,
        })
    }

    pub async fn run<Q, V>(&self, op: cynic::Operation<Q, V>) -> Result<cynic::GraphQlResponse<Q>>
    where
        Q: serde::de::DeserializeOwned + 'static,
        V: serde::Serialize,
    {
        // Simple single request without retry - retry logic can be added later if needed
        let result = self
            .client
            .post(&self.url)
            .header("Authorization", &self.api_key)
            .header("Content-Type", "application/json")
            .run_graphql(op)
            .await;

        result.map_err(|e| anyhow!(e))
    }
}
