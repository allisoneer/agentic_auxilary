//! V2 question API.

use crate::http::encode_path_segment;

#[derive(Clone)]
pub struct QuestionApi {
    http: super::V2HttpClient,
}

impl QuestionApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list_requests(
        &self,
    ) -> crate::error::Result<
        crate::types::v2::common::LocationResponse<
            Vec<crate::types::v2::question::QuestionRequest>,
        >,
    > {
        self.http.get("/api/question/request").await
    }

    pub async fn list_session_requests(
        &self,
        session_id: &str,
    ) -> crate::error::Result<
        crate::types::v2::common::DataResponse<Vec<crate::types::v2::question::QuestionRequest>>,
    > {
        let sid = encode_path_segment(session_id);
        self.http.get(&format!("/api/session/{sid}/question")).await
    }

    pub async fn reply(
        &self,
        session_id: &str,
        request_id: &str,
        req: &crate::types::v2::question::QuestionReply,
    ) -> crate::error::Result<()> {
        let sid = encode_path_segment(session_id);
        let rid = encode_path_segment(request_id);
        self.http
            .post_empty(&format!("/api/session/{sid}/question/{rid}/reply"), req)
            .await
    }

    pub async fn reject(&self, session_id: &str, request_id: &str) -> crate::error::Result<()> {
        let sid = encode_path_segment(session_id);
        let rid = encode_path_segment(request_id);
        self.http
            .post_empty(
                &format!("/api/session/{sid}/question/{rid}/reject"),
                &serde_json::json!({}),
            )
            .await
    }
}
