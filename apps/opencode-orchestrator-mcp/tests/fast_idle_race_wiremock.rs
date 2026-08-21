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
use wiremock::matchers::query_param;

use support::SequenceResponder;
use support::message_fixture;
use support::message_history_fixture;
use support::messages_fixture;
use support::patch_file_metadata_fixture;
use support::permission_fixture;
use support::permission_fixture_with_metadata;
use support::permission_patch_file_array_bad_request_fixture;
use support::session_fixture;
use support::session_fixture_with_parent;
use support::short_timeout_test_orchestrator_server;
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

async fn assert_command_dispatch_invalid_input(status: u16, body: serde_json::Value) {
    let _guard = env_lock().await;
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = format!("command-invalid-input-{status}");

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(&sid)))
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

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_history_fixture(vec![])))
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
        .and(path(format!("/session/{sid}/command")))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&mock)
        .await;

    let err = timeout(
        Duration::from_secs(5),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.clone()),
                command: Some("implement_plan".into()),
                agent: None,
                message: Some("args".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("invalid command dispatch regression should not hang")
    .expect_err("400/404 command dispatch should be invalid input");

    match err {
        ToolError::InvalidInput(message) => {
            let lower = message.to_lowercase();
            assert!(lower.contains("failed to dispatch command 'implement_plan'"));
            assert!(lower.contains("bad request") || lower.contains("not found"));
        }
        other => panic!("expected invalid input error, got {other:?}"),
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

#[tokio::test]
async fn idle_grace_deadline_rechecks_descendant_blocker_before_finalizing() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "50") };

    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-idle-grace-recheck";
    let child = "child-idle-grace-recheck";
    let request_id = "permission-idle-grace-recheck";

    Mock::given(method("GET"))
        .and(path(format!("/session/{root}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{child}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(session_fixture_with_parent(child, Some(root))),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
                request_id,
                child,
                "bash",
                &["*"]
            )])),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/session/{root}/prompt_async")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{root}/message")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(root, Some("MUST_NOT_FINALIZE"))),
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

    let output = timeout(
        Duration::from_secs(2),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(root.to_string()),
                command: None,
                agent: None,
                message: Some("start work".to_string()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("idle-grace blocker recheck should not hang")
    .expect("idle-grace blocker recheck should succeed");

    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(output.permission_request_id.as_deref(), Some(request_id));
    assert!(output.response.is_none());
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
        .and(path("/permission"))
        .and(query_param("directory", "/tmp"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(400)
                .set_body_json(permission_patch_file_array_bad_request_fixture()),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ]))
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

    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
    ]);
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("PRE_REPLY_DONE"))),
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
        requests
            .iter()
            .any(|request| request.url.path() == format!("/permission/{perm_id}/reply")),
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
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path("/permission"))
        .and(query_param("directory", "/tmp"))
        .respond_with(permission_seq)
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
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(sid, Some("POST_REPLY_DONE"))),
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
        requests
            .iter()
            .any(|request| request.url.path() == format!("/permission/{perm_id}/reply")),
        "reply POST should be observed before the follow-up failure: {:?}",
        requests
            .iter()
            .map(|request| request.url.path().to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn run_tolerates_initial_permission_list_bad_request_with_warning() {
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

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/permission"))
        .and(query_param("directory", "/tmp"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(400)
                .set_body_json(permission_patch_file_array_bad_request_fixture()),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ]))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("OK"))))
        .mount(&mock)
        .await;

    let result = timeout(
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
    .expect("degraded-success regression should not hang")
    .expect("run should continue with a warning on initial permission-list 400");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("OK"));
    assert!(result.warnings.iter().any(|warning| {
        let warning = warning.to_lowercase();
        warning.contains("could not be listed")
            && warning.contains("stale")
            && warning.contains("malformed")
    }));
}

#[tokio::test]
async fn respond_permission_no_id_permission_list_400_is_actionable_invalid_input() {
    let _guard = env_lock().await;
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "permission-discovery-400";

    Mock::given(method("GET"))
        .and(path("/permission"))
        .and(query_param("directory", "/tmp"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(permission_patch_file_array_bad_request_fixture()),
        )
        .mount(&mock)
        .await;

    let err = tool
        .call(
            RespondPermissionInput {
                session_id: sid.into(),
                permission_request_id: None,
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("no-id discovery should surface actionable invalid input");

    match err {
        ToolError::InvalidInput(message) => {
            let message = message.to_lowercase();
            assert!(message.contains("could not be listed"));
            assert!(message.contains("permission_request_id"));
        }
        other => panic!("expected invalid input error, got {other:?}"),
    }
}

#[tokio::test]
async fn respond_permission_reply_404_is_actionable_invalid_input() {
    let _guard = env_lock().await;
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "permission-reply-404";
    let perm_id = "perm-stale-404";

    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture(perm_id, sid, "file.write", &["/tmp/out.txt"])
        ])))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/permission/{perm_id}/reply")))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "name": "NotFound",
            "message": "Permission request not found"
        })))
        .mount(&mock)
        .await;

    let err = tool
        .call(
            RespondPermissionInput {
                session_id: sid.into(),
                permission_request_id: None,
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("stale reply should surface actionable invalid input");

    match err {
        ToolError::InvalidInput(message) => {
            let message = message.to_lowercase();
            assert!(message.contains(perm_id));
            assert!(message.contains("not found") || message.contains("no longer pending"));
        }
        other => panic!("expected invalid input error, got {other:?}"),
    }

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    assert!(
        requests
            .iter()
            .any(|request| request.url.path() == format!("/permission/{perm_id}/reply")),
        "reply POST should be observed for stale permission id: {:?}",
        requests
            .iter()
            .map(|request| request.url.path().to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn command_transport_error_after_start_evidence_warns_and_completes() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "0") };

    let mock = MockServer::start().await;
    let server = short_timeout_test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "command-transport-post-start";

    let baseline_messages =
        message_history_fixture(vec![message_fixture(sid, "u0", "user", 1, None, vec![])]);
    let transcript_with_command = message_history_fixture(vec![
        message_fixture(sid, "u0", "user", 1, None, vec![]),
        message_fixture(sid, "cmd-user", "user", 3, None, vec![]),
    ]);
    let completed_messages = message_history_fixture(vec![
        message_fixture(sid, "u0", "user", 1, None, vec![]),
        message_fixture(sid, "cmd-user", "user", 3, None, vec![]),
        message_fixture(
            sid,
            "a1",
            "assistant",
            4,
            Some(4),
            vec![serde_json::json!({"type": "text", "text": "COMMAND_DONE"})],
        ),
    ]);

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
            ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
            ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
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
        .and(path(format!("/session/{sid}/message")))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(baseline_messages),
            ResponseTemplate::new(200).set_body_json(transcript_with_command),
            ResponseTemplate::new(200).set_body_json(completed_messages),
        ]))
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
        .and(path(format!("/session/{sid}/command")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "executed"}))
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    let result = timeout(
        Duration::from_secs(5),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: Some("implement_plan".into()),
                agent: None,
                message: Some("args".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("post-start transport regression should not hang")
    .expect("post-start transport regression should complete with warning");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("COMMAND_DONE"));
    assert!(result.warnings.iter().any(|warning| {
        warning.contains("transport error") && warning.contains("continuing supervision")
    }));

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    let command_posts = requests
        .iter()
        .filter(|request| request.url.path() == format!("/session/{sid}/command"))
        .count();
    assert_eq!(
        command_posts, 1,
        "transport ambiguity must not trigger command retry"
    );
}

#[tokio::test]
async fn command_transport_error_before_start_evidence_fails_clearly() {
    let _guard = env_lock().await;
    let _env = EnvVarGuard(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS);
    // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
    unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "0") };

    let mock = MockServer::start().await;
    let server = short_timeout_test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "command-transport-pre-start";

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
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
        .and(path(format!("/session/{sid}/message")))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(message_history_fixture(vec![])),
            ResponseTemplate::new(200).set_body_json(message_history_fixture(vec![])),
        ]))
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
        .and(path(format!("/session/{sid}/command")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "executed"}))
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    let err = timeout(
        Duration::from_secs(5),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: Some("implement_plan".into()),
                agent: None,
                message: Some("args".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("pre-start transport regression should not hang")
    .expect_err("pre-start transport regression should fail clearly");

    match err {
        ToolError::Internal(message) => {
            let lower = message.to_lowercase();
            assert!(lower.contains("transport error before session start evidence"));
            assert!(lower.contains("failed to dispatch command 'implement_plan'"));
        }
        other => panic!("expected internal dispatch failure, got {other:?}"),
    }

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    let command_posts = requests
        .iter()
        .filter(|request| request.url.path() == format!("/session/{sid}/command"))
        .count();
    assert_eq!(
        command_posts, 1,
        "transport ambiguity must not trigger command retry"
    );
}

#[tokio::test]
async fn command_dispatch_400_is_actionable_invalid_input() {
    assert_command_dispatch_invalid_input(
        400,
        serde_json::json!({"name": "BadRequest", "message": "Bad request"}),
    )
    .await;
}

#[tokio::test]
async fn command_dispatch_404_is_actionable_invalid_input() {
    assert_command_dispatch_invalid_input(
        404,
        serde_json::json!({"name": "NotFound", "message": "Not found"}),
    )
    .await;
}

#[tokio::test]
async fn command_dispatch_posts_server_valid_message_id() {
    let _guard = env_lock().await;
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "command-valid-message-id";

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
    ]);
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
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
        .and(path(format!("/session/{sid}/message")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("COMMAND_DONE"))),
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

    Mock::given(method("POST"))
        .and(path(format!("/session/{sid}/command")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "ok"})))
        .mount(&mock)
        .await;

    let result = timeout(
        Duration::from_secs(5),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: Some("implement_plan".into()),
                agent: None,
                message: Some("args".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("command dispatch message-id regression should not hang")
    .expect("command dispatch should succeed");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("COMMAND_DONE"));

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    let command_request = requests
        .iter()
        .find(|request| {
            request.method.as_str() == "POST"
                && request.url.path() == format!("/session/{sid}/command")
        })
        .expect("wiremock should capture command POST");
    let body: serde_json::Value =
        serde_json::from_slice(&command_request.body).expect("command request body should be JSON");
    let message_id = body
        .get("messageID")
        .and_then(serde_json::Value::as_str)
        .expect("command request should include messageID");
    assert!(
        message_id.starts_with("msg"),
        "expected server-valid messageID prefix, got {message_id}"
    );
}
