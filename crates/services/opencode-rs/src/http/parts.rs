//! Parts API for `OpenCode`.
//!
//! Endpoints for modifying message parts.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::api::UpdatePartResponse;
use reqwest::Method;

/// Parts API client.
#[derive(Clone)]
pub struct PartsApi {
    http: HttpClient,
}

impl PartsApi {
    /// Create a new Parts API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Delete a part from a message.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn delete(&self, session_id: &str, message_id: &str, part_id: &str) -> Result<()> {
        self.http
            .request_empty(
                Method::DELETE,
                &format!("/session/{session_id}/message/{message_id}/part/{part_id}"),
                None,
            )
            .await
    }

    /// Update a part in a message.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn update(
        &self,
        session_id: &str,
        message_id: &str,
        part_id: &str,
        body: &serde_json::Value,
    ) -> Result<UpdatePartResponse> {
        self.http
            .request_json(
                Method::PATCH,
                &format!("/session/{session_id}/message/{message_id}/part/{part_id}"),
                Some(body.clone()),
            )
            .await
    }
}
