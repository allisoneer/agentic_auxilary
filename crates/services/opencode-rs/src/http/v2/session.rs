//! V2 session API.

use crate::http::encode_path_segment;

#[derive(Clone)]
pub struct SessionApi {
    http: super::V2HttpClient,
}

impl SessionApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list(
        &self,
        params: &crate::types::v2::session::SessionListParams,
    ) -> crate::error::Result<
        crate::types::v2::common::CursorResponse<crate::types::v2::session::SessionV2Info>,
    > {
        self.http
            .get_with_query("/api/session", &params.to_query_pairs())
            .await
    }

    pub async fn create(
        &self,
        req: &crate::types::v2::session::CreateSessionRequest,
    ) -> crate::error::Result<
        crate::types::v2::common::DataResponse<crate::types::v2::session::SessionV2Info>,
    > {
        self.http.post("/api/session", req).await
    }

    pub async fn get(
        &self,
        session_id: &str,
    ) -> crate::error::Result<
        crate::types::v2::common::DataResponse<crate::types::v2::session::SessionV2Info>,
    > {
        let sid = encode_path_segment(session_id);
        self.http.get(&format!("/api/session/{sid}")).await
    }

    pub async fn prompt(
        &self,
        session_id: &str,
        req: &crate::types::v2::session::SessionPromptRequest,
    ) -> crate::error::Result<
        crate::types::v2::common::DataResponse<crate::types::v2::session::SessionInputAdmitted>,
    > {
        let sid = encode_path_segment(session_id);
        self.http
            .post(&format!("/api/session/{sid}/prompt"), req)
            .await
    }

    pub async fn compact(&self, session_id: &str) -> crate::error::Result<()> {
        let sid = encode_path_segment(session_id);
        self.http
            .post_empty(
                &format!("/api/session/{sid}/compact"),
                &serde_json::json!({}),
            )
            .await
    }

    pub async fn wait(&self, session_id: &str) -> crate::error::Result<()> {
        let sid = encode_path_segment(session_id);
        self.http
            .post_empty(&format!("/api/session/{sid}/wait"), &serde_json::json!({}))
            .await
    }

    pub async fn context(
        &self,
        session_id: &str,
    ) -> crate::error::Result<crate::types::v2::common::DataResponse<Vec<serde_json::Value>>> {
        let sid = encode_path_segment(session_id);
        self.http.get(&format!("/api/session/{sid}/context")).await
    }
}
