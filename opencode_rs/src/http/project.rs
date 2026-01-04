//! Project API for OpenCode.
//!
//! Endpoints for project management.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::project::{Project, UpdateProjectRequest};
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
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::PATCH,
                &format!("/project/{}", project_id),
                Some(body),
            )
            .await
    }
}
