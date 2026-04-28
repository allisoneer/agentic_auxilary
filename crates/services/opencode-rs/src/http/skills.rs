//! Skills API for `OpenCode`.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::skill::Skill;
use reqwest::Method;

/// Skills API client.
#[derive(Clone)]
pub struct SkillsApi {
    http: HttpClient,
}

impl SkillsApi {
    /// Create a new Skills API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List all available skills.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<Skill>> {
        self.http.request_json(Method::GET, "/skill", None).await
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
    async fn test_list_skills() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/skill"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "name": "code-review",
                    "description": "Review code",
                    "location": "/skills/code-review/SKILL.md",
                    "content": "Review the supplied code."
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            workspace: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SkillsApi::new(client);
        let skills = api.list().await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "code-review");
        assert_eq!(skills[0].location, "/skills/code-review/SKILL.md");
    }
}
