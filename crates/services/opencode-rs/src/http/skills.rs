//! Skills API for `OpenCode`.
//!
//! Endpoints for managing skills (reusable prompt templates).

use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::skill::SkillDirs;
use crate::types::skill::SkillInfo;
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
    pub async fn list(&self) -> Result<Vec<SkillInfo>> {
        self.http
            .request_json(Method::GET, "/skill/list", None)
            .await
    }

    /// Get a specific skill by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or skill is not found.
    pub async fn get(&self, name: &str) -> Result<SkillInfo> {
        let encoded_name = encode_path_segment(name);
        self.http
            .request_json(
                Method::GET,
                &format!("/skill/get?name={encoded_name}"),
                None,
            )
            .await
    }

    /// Get skill directories.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn dirs(&self) -> Result<SkillDirs> {
        self.http
            .request_json(Method::GET, "/skill/dirs", None)
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
    use wiremock::matchers::query_param;

    #[tokio::test]
    async fn test_list_skills() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/skill/list"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"name": "code-review", "description": "Review code", "builtin": true},
                {"name": "refactor", "description": "Refactor code", "builtin": false}
            ])))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SkillsApi::new(client);
        let skills = api.list().await.unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "code-review");
        assert!(skills[0].builtin);
    }

    #[tokio::test]
    async fn test_get_skill() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/skill/get"))
            .and(query_param("name", "code-review"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "code-review",
                "description": "Review code for issues",
                "content": "Please review the following code...",
                "builtin": true
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SkillsApi::new(client);
        let skill = api.get("code-review").await.unwrap();
        assert_eq!(skill.name, "code-review");
        assert!(skill.content.is_some());
    }

    #[tokio::test]
    async fn test_skill_dirs() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/skill/dirs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "dirs": ["/project/.opencode/skills", "/home/user/.opencode/skills"]
            })))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let api = SkillsApi::new(client);
        let dirs = api.dirs().await.unwrap();
        assert_eq!(dirs.dirs.len(), 2);
    }
}
