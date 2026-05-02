#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::tools::RespondPermissionTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use opencode_orchestrator_mcp::types::PermissionReply;
use opencode_orchestrator_mcp::types::RespondPermissionInput;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::path_regex;

use support::SequenceResponder;
use support::permission_fixture;
use support::session_fixture;
use support::status_v2_busy;
use support::test_orchestrator_server;

#[tokio::test]
async fn run_returns_cancelled_when_request_context_is_cancelled() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let session_id = "cancel-run";

    Mock::given(method("GET"))
        .and(path(format!("/session/{session_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(session_id)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy(session_id)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/session/{session_id}/command")))
        .respond_with(ResponseTemplate::new(204).set_delay(Duration::from_secs(30)))
        .mount(&mock)
        .await;

    let ctx = ToolContext::default();
    let cancel = ctx.cancellation_token();
    let handle = tokio::spawn(async move {
        tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.into()),
                command: Some("research".into()),
                message: Some("test".into()),
                wait_for_activity: None,
            },
            &ctx,
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();

    let result = timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result,
        Err(agentic_tools_core::ToolError::Cancelled { .. })
    ));
}

#[tokio::test]
async fn respond_permission_returns_cancelled_during_post_reply_monitoring() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let session_id = "cancel-perm";
    let permission_id = "perm-1";

    let permission_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
            permission_id,
            session_id,
            "write",
            &["src/**"]
        ),])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(permission_seq)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{session_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(session_id)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy(session_id)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    let ctx = ToolContext::default();
    let cancel = ctx.cancellation_token();
    let handle = tokio::spawn(async move {
        tool.call(
            RespondPermissionInput {
                session_id: session_id.into(),
                permission_request_id: Some(permission_id.into()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ctx,
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();

    let result = timeout(Duration::from_secs(2), handle)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result,
        Err(agentic_tools_core::ToolError::Cancelled { .. })
    ));
}
