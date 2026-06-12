//! V2 message API.

use crate::http::encode_path_segment;

#[derive(Clone)]
pub struct MessageApi {
    http: super::V2HttpClient,
}

impl MessageApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(
        &self,
        session_id: &str,
        cursor: Option<&str>,
    ) -> crate::error::Result<crate::types::v2::common::CursorResponse<serde_json::Value>> {
        let sid = encode_path_segment(session_id);
        let mut query = Vec::new();
        if let Some(cursor) = cursor {
            query.push(("cursor", cursor.to_string()));
        }
        self.http
            .get_with_query(&format!("/api/session/{sid}/message"), &query)
            .await
    }
}
