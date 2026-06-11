//! Wiremock regressions for fast-idle dispatch and resume races.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use opencode_orchestrator_mcp::config::OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::tools::RespondPermissionTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use opencode_orchestrator_mcp::types::PermissionReply;
use opencode_orchestrator_mcp::types::RespondPermissionInput;
use opencode_orchestrator_mcp::types::RunStatus;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::path_regex;

use support::SequenceResponder;
use support::messages_fixture;
use support::patch_file_metadata_fixture;
use support::permission_fixture;
use support::permission_fixture_with_metadata;
use support::permission_patch_file_array_bad_request_fixture;
use support::session_fixture;
use support::status_v2_busy;
use support::status_v2_idle;
use support::test_orchestrator_server;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

struct EnvVarGuard(&'static str);

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::remove_var(self.0) };
    }
}

#[tokio::test]
async fn fast_idle_prompt_completes_without_hanging() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "0") };

    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "fast-idle-prompt";

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/wait"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/session/.*/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/question/request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/api/session/{sid}/prompt")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data": {"id": "input-1"}})),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/api/session/{sid}/context")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("FAST_IDLE_DONE"))),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    let result = timeout(
        Duration::from_secs(2),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: None,
                agent: None,
                message: Some("say hello".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("fast-idle prompt should not hang")
    .expect("run should succeed");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("FAST_IDLE_DONE"));
}

#[tokio::test]
async fn fast_idle_resume_after_permission_reply_completes_without_hanging() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "0") };

    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "fast-idle-resume";
    let perm_id = "perm-fast-idle";

    let permission_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
            perm_id,
            sid,
            "file.write",
            &["/tmp/out.txt"],
        )])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path_regex(r"/api/session/.*/permission"))
        .respond_with(permission_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/question/request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/wait"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/api/session/{sid}/context")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("RESUME_DONE"))),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    let result = timeout(
        Duration::from_secs(2),
        tool.call(
            RespondPermissionInput {
                session_id: sid.into(),
                permission_request_id: None,
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("fast-idle resume should not hang")
    .expect("respond_permission should succeed");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("RESUME_DONE"));
}

#[tokio::test]
async fn respond_permission_known_id_replies_even_when_permission_list_bad_requests() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "0") };

    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "permission-pre-reply-wedge";
    let perm_id = "perm-patch-pre";

    Mock::given(method("GET"))
        .and(path_regex(r"/api/session/.*/permission"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(permission_patch_file_array_bad_request_fixture()),
        )
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
    ]);
    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/wait"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/question/request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/api/session/{sid}/context")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("PRE_REPLY_DONE"))),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    let result = timeout(
        Duration::from_secs(2),
        tool.call(
            RespondPermissionInput {
                session_id: sid.into(),
                permission_request_id: Some(perm_id.into()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("known-id continuation should not hang")
    .expect("respond_permission should succeed with provided request id");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("PRE_REPLY_DONE"));
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("Permission validation failed")),
        "expected validation warning, got {:?}",
        result.warnings
    );

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    assert!(
        requests.iter().any(|request| request.url.path()
            == format!("/api/session/{sid}/permission/{perm_id}/reply")),
        "reply POST should be observed with a known request id: {:?}",
        requests
            .iter()
            .map(|request| request.url.path().to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn respond_permission_continues_after_reply_when_follow_up_permission_list_bad_requests() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "0") };

    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "permission-post-reply-wedge";
    let perm_id = "perm-patch-post";

    let permission_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture_with_metadata(
                perm_id,
                sid,
                "edit",
                &["src/lib.rs"],
                &serde_json::json!({"files": [patch_file_metadata_fixture()]}),
            )
        ])),
        ResponseTemplate::new(400).set_body_json(permission_patch_file_array_bad_request_fixture()),
    ]);
    Mock::given(method("GET"))
        .and(path_regex(r"/api/session/.*/permission"))
        .respond_with(permission_seq)
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/wait"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/question/request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/api/session/{sid}/context")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(sid, Some("POST_REPLY_DONE"))),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    let result = timeout(
        Duration::from_secs(2),
        tool.call(
            RespondPermissionInput {
                session_id: sid.into(),
                permission_request_id: Some(perm_id.into()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("post-reply continuation should not hang")
    .expect("respond_permission should keep monitoring after the follow-up 400");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("POST_REPLY_DONE"));
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("Permission refresh failed after reply")),
        "expected continuation warning, got {:?}",
        result.warnings
    );

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    assert!(
        requests.iter().any(|request| request.url.path()
            == format!("/api/session/{sid}/permission/{perm_id}/reply")),
        "reply POST should be observed before the follow-up failure: {:?}",
        requests
            .iter()
            .map(|request| request.url.path().to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn run_still_errors_on_initial_permission_list_bad_request() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "0") };

    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "permission-strict-run";

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/wait"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/session/.*/permission"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(permission_patch_file_array_bad_request_fixture()),
        )
        .mount(&mock)
        .await;

    let err = timeout(
        Duration::from_secs(2),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: None,
                agent: None,
                message: None,
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("strict run regression should not hang")
    .expect_err("run should remain strict on initial permission-list failures");

    assert!(matches!(err, ToolError::Internal(_)));
}
