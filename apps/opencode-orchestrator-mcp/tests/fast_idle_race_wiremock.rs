//! Wiremock regressions for fast-idle dispatch and resume races.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
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
use support::permission_fixture;
use support::session_fixture;
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
    let server = test_orchestrator_server(&mock);
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "fast-idle-prompt";

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
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

    Mock::given(method("POST"))
        .and(path(format!("/session/{sid}/prompt_async")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("FAST_IDLE_DONE"))),
        )
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

    let result = timeout(
        Duration::from_secs(2),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: None,
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
    let server = test_orchestrator_server(&mock);
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
        .and(path("/permission"))
        .respond_with(permission_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("RESUME_DONE"))),
        )
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
