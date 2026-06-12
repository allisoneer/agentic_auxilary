//! V2 permission API.

use crate::http::encode_path_segment;

#[derive(Clone)]
pub struct PermissionApi {
    http: super::V2HttpClient,
}

impl PermissionApi {
    pub(crate) fn new(http: super::V2HttpClient) -> Self {
        Self { http }
    }

    pub async fn list_requests(
        &self,
    ) -> crate::error::Result<
        crate::types::v2::common::LocationResponse<
            Vec<crate::types::v2::permission::PermissionRequest>,
        >,
    > {
        self.http.get("/api/permission/request").await
    }

    pub async fn list_session_requests(
        &self,
        session_id: &str,
    ) -> crate::error::Result<
        crate::types::v2::common::DataResponse<
            Vec<crate::types::v2::permission::PermissionRequest>,
        >,
    > {
        let sid = encode_path_segment(session_id);
        self.http
            .get(&format!("/api/session/{sid}/permission"))
            .await
    }

    pub async fn reply(
        &self,
        session_id: &str,
        request_id: &str,
        req: &crate::types::v2::permission::PermissionReplyRequest,
    ) -> crate::error::Result<()> {
        let sid = encode_path_segment(session_id);
        let rid = encode_path_segment(request_id);
        self.http
            .post_empty(&format!("/api/session/{sid}/permission/{rid}/reply"), req)
            .await
    }
}
