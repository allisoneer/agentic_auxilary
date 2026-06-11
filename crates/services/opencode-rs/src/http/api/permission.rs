use crate::error::Result;
use crate::http::HttpClient;
use crate::http::encode_path_segment;
use crate::types::v2::envelope::DataEnvelope;
use crate::types::v2::envelope::LocationEnvelope;
use crate::types::v2::permission::PermissionReplyRequest;
use crate::types::v2::permission::PermissionRequest;
#[derive(Clone)]
pub struct PermissionApi {
    http: HttpClient,
}

impl PermissionApi {
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    pub async fn list_requests(&self) -> Result<LocationEnvelope<Vec<PermissionRequest>>> {
        self.http.api_get("/api/permission/request").await
    }

    pub async fn list_session(
        &self,
        session_id: &str,
    ) -> Result<DataEnvelope<Vec<PermissionRequest>>> {
        let session_id = encode_path_segment(session_id);
        self.http
            .api_get(&format!("/api/session/{session_id}/permission"))
            .await
    }

    pub async fn reply(
        &self,
        session_id: &str,
        request_id: &str,
        request: &PermissionReplyRequest,
    ) -> Result<()> {
        let session_id = encode_path_segment(session_id);
        let request_id = encode_path_segment(request_id);
        self.http
            .api_post_empty(
                &format!("/api/session/{session_id}/permission/{request_id}/reply"),
                request,
            )
            .await
    }
}
