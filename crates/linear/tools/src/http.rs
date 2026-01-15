use anyhow::{Result, anyhow};
use cynic::http::ReqwestExt;
use reqwest::Client;

pub struct LinearClient {
    client: Client,
    url: String,
    api_key: String,
}

/// Centralized GraphQL error extraction - fails fast on any errors
pub fn extract_data<Q>(resp: cynic::GraphQlResponse<Q>) -> Result<Q> {
    if let Some(errors) = resp.errors
        && !errors.is_empty()
    {
        let mut parts = Vec::new();
        for e in errors {
            let path = e.path.unwrap_or_default();
            let path_str = if path.is_empty() {
                String::new()
            } else {
                let p = path
                    .into_iter()
                    .map(|v| match v {
                        cynic::GraphQlErrorPathSegment::Field(f) => f,
                        cynic::GraphQlErrorPathSegment::Index(i) => i.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(".");
                format!(" (path: {})", p)
            };
            parts.push(format!("{}{}", e.message, path_str));
        }
        return Err(anyhow!(
            "GraphQL errors from Linear:\n- {}",
            parts.join("\n- ")
        ));
    }

    match resp.data {
        Some(data) => Ok(data),
        None => Err(anyhow!("No data returned from Linear")),
    }
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
        let mut req = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json");

        // Auto-detect auth header type:
        // - Personal API key: "lin_api_*" => raw Authorization header
        // - OAuth2 token: anything else => Bearer token
        if self.api_key.starts_with("lin_api_") {
            req = req.header("Authorization", &self.api_key);
        } else {
            req = req.bearer_auth(&self.api_key);
        }

        let result = req.run_graphql(op).await;
        result.map_err(|e| anyhow!(e))
    }
}
