//! Project API for `OpenCode`.
//!
//! Endpoints for project management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::project::GitInitRequest;
use crate::types::project::GitInitResponse;
use crate::types::project::Project;
use crate::types::project::UpdateProjectRequest;
use reqwest::Method;

/// Project API client.
#[derive(Clone)]
pub struct ProjectApi {
    http: HttpClient,
}

impl ProjectApi {
    /// Create a new Project API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List projects.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<Project>> {
        self.http.request_json(Method::GET, "/project", None).await
    }

    /// Get current project.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn current(&self) -> Result<Project> {
        self.http
            .request_json(Method::GET, "/project/current", None)
            .await
    }

    /// Update a project.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn update(&self, project_id: &str, req: &UpdateProjectRequest) -> Result<Project> {
        let pid = encode_path_segment(project_id);
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::PATCH, &format!("/project/{pid}"), Some(body))
            .await
    }

    /// Initialize a git repository in the project directory.
    ///
    /// # Errors
    ///
    /// Returns an error if git initialization fails.
    pub async fn git_init(&self, req: &GitInitRequest) -> Result<GitInitResponse> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(Method::POST, "/project/git/init", Some(body))
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
    async fn test_list_projects_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/project"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "proj1",
                    "name": "Project One"
                }
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let project = ProjectApi::new(http);
        let result = project.list().await;
        assert!(result.is_ok());
        let projects = result.unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[tokio::test]
    async fn test_current_project_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/project/current"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "current-proj",
                "name": "Current Project"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let project = ProjectApi::new(http);
        let result = project.current().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_project_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/project/proj1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "proj1",
                "name": "Updated Project"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let project = ProjectApi::new(http);
        let result = project
            .update(
                "proj1",
                &UpdateProjectRequest {
                    name: Some("Updated Project".to_string()),
                    settings: None,
                },
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_git_init_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/project/git/init"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "path": "/home/user/project/.git"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let project = ProjectApi::new(http);
        let result = project
            .git_init(&GitInitRequest {
                default_branch: Some("main".to_string()),
            })
            .await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_git_init_already_initialized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/project/git/init"))
            .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
                "name": "Conflict",
                "message": "Git repository already initialized"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let project = ProjectApi::new(http);
        let result = project
            .git_init(&GitInitRequest {
                default_branch: None,
            })
            .await;
        assert!(result.is_err());
    }
}
