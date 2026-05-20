use agentic_config::types::LinearServiceConfig;
use anyhow::Result;
use anyhow::anyhow;
use cynic::http::ReqwestExt;
use reqwest::Client;
use std::time::Duration;

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
                format!(" (path: {p})")
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
    pub fn new(api_key: Option<String>, config: &LinearServiceConfig) -> Result<Self> {
        let api_key = match api_key.or_else(|| std::env::var("LINEAR_API_KEY").ok()) {
            Some(k) if !k.is_empty() => k,
            _ => return Err(anyhow!("LINEAR_API_KEY environment variable is not set")),
        };

        let url = std::env::var("LINEAR_GRAPHQL_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| config.base_url.clone());

        let mut builder = Client::builder().user_agent("linear-tools/0.1.0");
        if config.connect_timeout_secs != 0 {
            builder = builder.connect_timeout(Duration::from_secs(config.connect_timeout_secs));
        }
        if config.request_timeout_secs != 0 {
            builder = builder.timeout(Duration::from_secs(config.request_timeout_secs));
        }
        let client = builder.build()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    struct EnvGuard(&'static str);

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: These tests run under #[serial], so no concurrent env access occurs.
            unsafe {
                std::env::remove_var(self.0);
            }
        }
    }

    #[test]
    #[serial]
    fn new_uses_configured_base_url_and_zero_timeouts() {
        // SAFETY: This test runs under #[serial], so no concurrent env access occurs.
        unsafe {
            std::env::remove_var("LINEAR_API_KEY");
            std::env::remove_var("LINEAR_GRAPHQL_URL");
        }
        let _g1 = EnvGuard("LINEAR_API_KEY");
        let _g2 = EnvGuard("LINEAR_GRAPHQL_URL");

        let config = LinearServiceConfig {
            base_url: "https://linear.example/graphql".into(),
            connect_timeout_secs: 0,
            request_timeout_secs: 0,
        };

        let client = LinearClient::new(Some("token".into()), &config).unwrap();
        assert_eq!(client.url, "https://linear.example/graphql");
    }

    #[test]
    #[serial]
    fn new_preserves_legacy_env_override_for_url() {
        // SAFETY: This test runs under #[serial], so no concurrent env access occurs.
        unsafe {
            std::env::set_var("LINEAR_GRAPHQL_URL", "https://env.example/graphql");
        }
        let _guard = EnvGuard("LINEAR_GRAPHQL_URL");

        let config = LinearServiceConfig::default();
        let client = LinearClient::new(Some("token".into()), &config).unwrap();
        assert_eq!(client.url, "https://env.example/graphql");
    }
}
