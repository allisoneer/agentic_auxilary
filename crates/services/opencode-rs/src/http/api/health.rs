use crate::error::Result;
use crate::http::HttpClient;
use crate::types::v2::health::HealthInfo;

#[derive(Clone)]
pub struct HealthApi {
    http: HttpClient,
}

impl HealthApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn get(&self) -> Result<HealthInfo> {
        self.http.api_get("/api/health").await
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
    async fn parses_api_health_response() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "healthy": true
            })))
            .mount(&mock_server)
            .await;

        let api = HealthApi::new(
            HttpClient::new(HttpConfig {
                base_url: mock_server.uri(),
                directory: None,
                workspace: None,
                timeout: Duration::from_secs(30),
            })
            .unwrap(),
        );

        let response = api.get().await.unwrap();
        assert!(response.healthy);
    }
}
