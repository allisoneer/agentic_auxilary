//! Wiremock regressions for fast-idle dispatch and resume races.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use opencode_orchestrator_mcp::config::OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS;
use opencode_orchestrator_mcp::server::OrchestratorServer;
use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::server::RecoveryMode;
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
use support::status_v2_busy;
use support::status_v2_idle;
use support::test_orchestrator_server;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

async fn short_timeout_server(mock: &MockServer) -> Arc<OrchestratorServerHandle> {
    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "healthy": true,
            "version": "test",
        })))
        .mount(mock)
        .await;

    let base_url = mock.uri().trim_end_matches('/').to_string();
    let client = opencode_rs::ClientBuilder::new()
        .base_url(&base_url)
        .directory("/tmp".to_string())
        .timeout_secs(1)
        .build()
        .expect("short-timeout test client should build");

    Arc::new(OrchestratorServerHandle::from_server_unshared(
        OrchestratorServer::from_client_unshared(client, &base_url, RecoveryMode::External),
    ))
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

    let err = tool
        .call(
            OrchestratorRunInput {
                session_id: Some(sid.clone()),
                command: Some("implement_plan".into()),
                agent: None,
                message: Some("args".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
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
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(permission_patch_file_array_bad_request_fixture()),
        )
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
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(permission_patch_file_array_bad_request_fixture()),
        )
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
    let server = short_timeout_server(&mock).await;
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
    let server = short_timeout_server(&mock).await;
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
