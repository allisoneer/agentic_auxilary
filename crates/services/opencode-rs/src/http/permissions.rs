//! Permissions API for OpenCode.
//!
//! Endpoints for managing permission requests.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::api::PermissionReplyResponse;
use crate::types::permission::{PermissionReplyRequest, PermissionRequest};
use reqwest::Method;

/// Permissions API client.
#[derive(Clone)]
pub struct PermissionsApi {
    http: HttpClient,
}

impl PermissionsApi {
    /// Create a new Permissions API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List pending permission requests.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<PermissionRequest>> {
        self.http
            .request_json(Method::GET, "/permission", None)
            .await
    }

    /// Reply to a permission request.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn reply(
        &self,
        request_id: &str,
        reply: &PermissionReplyRequest,
    ) -> Result<PermissionReplyResponse> {
        let body = serde_json::to_value(reply)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/permission/{}/reply", request_id),
                Some(body),
            )
            .await
    }
}
