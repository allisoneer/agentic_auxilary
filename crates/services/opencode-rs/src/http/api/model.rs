use crate::error::Result;
use crate::http::HttpClient;
use crate::types::v2::envelope::LocationEnvelope;
use crate::types::v2::model::ModelInfo;

#[derive(Clone)]
pub struct ModelApi {
    http: HttpClient,
}

impl ModelApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(&self) -> Result<LocationEnvelope<Vec<ModelInfo>>> {
        self.http.api_get("/api/model").await
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
    async fn parses_location_wrapped_model_list() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/model"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "location": {
                    "directory": "/tmp/project",
                    "workspaceID": "ws-1"
                },
                "data": [
                    {
                        "id": "claude-sonnet-4",
                        "providerID": "anthropic",
                        "limit": {"context": 200_000}
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let api = ModelApi::new(
            HttpClient::new(HttpConfig {
                base_url: mock_server.uri(),
                directory: None,
                workspace: None,
                timeout: Duration::from_secs(30),
            })
            .unwrap(),
        );

        let response = api.list().await.unwrap();
        assert_eq!(response.location.directory, "/tmp/project");
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].provider_id, "anthropic");
        assert_eq!(
            response.data[0]
                .limit
                .as_ref()
                .and_then(|limit| limit.context),
            Some(200_000)
        );
    }
}
