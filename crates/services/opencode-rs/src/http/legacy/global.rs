use crate::error::Result;
use crate::http::HttpClient;
use crate::http::misc::HealthInfo;
use reqwest::Method;

#[derive(Clone)]
pub struct GlobalApi {
    http: HttpClient,
}

impl GlobalApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// `LEGACY_EXCEPTION(OpenCode v1.17.2)`: startup-only exact version validation still requires `/global/health`.
    pub async fn health(&self) -> Result<HealthInfo> {
        self.http
            .request_json(Method::GET, "/global/health", None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn supports_legacy_global_health() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/global/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "healthy": true,
                "version": "1.17.2"
            })))
            .mount(&mock_server)
            .await;

        let api = GlobalApi::new(
            HttpClient::new(HttpConfig {
                base_url: mock_server.uri(),
                directory: None,
                workspace: None,
                timeout: Duration::from_secs(30),
            })
            .unwrap(),
        );

        let response = api.health().await.unwrap();
        assert!(response.healthy);
        assert_eq!(response.version.as_deref(), Some("1.17.2"));
    }
}
