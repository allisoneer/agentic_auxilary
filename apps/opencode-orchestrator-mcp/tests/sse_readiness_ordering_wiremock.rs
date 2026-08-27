#![expect(clippy::expect_used)]

mod support;

use agentic_config::types::OrchestratorConfig;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use opencode_orchestrator_mcp::tools::ListCommandsTool;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::tools::RespondPermissionTool;
use opencode_orchestrator_mcp::tools::RespondQuestionTool;
use opencode_orchestrator_mcp::types::ListCommandsInput;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use opencode_orchestrator_mcp::types::PermissionReply;
use opencode_orchestrator_mcp::types::QuestionAction;
use opencode_orchestrator_mcp::types::RespondPermissionInput;
use opencode_orchestrator_mcp::types::RespondQuestionInput;
use opencode_orchestrator_mcp::types::RunStatus;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use support::CallCounter;
use support::SequenceResponder;
use support::messages_fixture;
use support::permission_fixture;
use support::question_fixture;
use support::session_fixture;
use support::status_v2_busy;
use support::status_v2_idle;
use support::test_orchestrator_server;
use support::test_orchestrator_server_with_config;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

fn question_payload(prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "question": prompt,
        "header": "Readiness question",
        "options": [{"label": "yes", "description": "Proceed"}],
        "multiple": false,
        "custom": false,
    })
}

async fn mount_session(mock: &MockServer, session_id: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/session/{session_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(session_id)))
        .mount(mock)
        .await;
}

async fn mount_empty_permissions(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(mock)
        .await;
}

async fn mount_empty_questions(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(mock)
        .await;
}

async fn mount_messages(mock: &MockServer, session_id: &str, response: Option<&str>) {
    Mock::given(method("GET"))
        .and(path(format!("/session/{session_id}/message")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(session_id, response)),
        )
        .mount(mock)
        .await;
}

async fn mount_run_statuses(mock: &MockServer, session_id: &str) {
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
            ResponseTemplate::new(200).set_body_json(status_v2_busy(session_id)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(mock)
        .await;
}

async fn mount_barrier(mock: &MockServer) -> support::SseReadinessBarrierControl {
    let (responder, control) = support::sse_readiness_barrier();
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(responder)
        .mount(mock)
        .await;
    control
}

fn counted_response(template: ResponseTemplate) -> (SequenceResponder, CallCounter) {
    let responder = SequenceResponder::new(vec![template]);
    let counter = responder.call_counter();
    (responder, counter)
}

async fn assert_single_snapshot(mock: &MockServer) {
    let requests = mock
        .received_requests()
        .await
        .expect("request log unavailable");
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/global/health")
            .count(),
        1,
        "tool call should acquire exactly one server snapshot"
    );
}

async fn run_action_is_gated(command: bool) {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let session_id = if command {
        "readiness-command"
    } else {
        "readiness-prompt"
    };

    mount_session(&mock, session_id).await;
    mount_run_statuses(&mock, session_id).await;
    mount_empty_permissions(&mock).await;
    mount_empty_questions(&mock).await;
    mount_messages(&mock, session_id, Some("READY_DONE")).await;
    let mut barrier = mount_barrier(&mock).await;

    let action_path = if command {
        format!("/session/{session_id}/command")
    } else {
        format!("/session/{session_id}/prompt_async")
    };
    let action_response = if command {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "ok"}))
    } else {
        ResponseTemplate::new(204)
    };
    let (action_responder, action_calls) = counted_response(action_response);
    Mock::given(method("POST"))
        .and(path(action_path))
        .respond_with(action_responder)
        .mount(&mock)
        .await;

    let input = OrchestratorRunInput {
        session_id: Some(session_id.into()),
        command: command.then(|| "implement_plan".into()),
        agent: None,
        message: Some("do the work".into()),
        wait_for_activity: None,
    };
    let handle = tokio::spawn(async move { tool.call(input, &ToolContext::default()).await });

    barrier.wait_for_arrival().await;
    assert_eq!(action_calls.get(), 0, "action was sent before SSE Open");
    barrier.release();

    let output = timeout(TEST_TIMEOUT, handle)
        .await
        .expect("run action did not complete")
        .expect("run task panicked")
        .expect("run action failed");
    assert!(matches!(output.status, RunStatus::Completed));
    assert_eq!(output.response.as_deref(), Some("READY_DONE"));
    assert!(output.warnings.is_empty());
    assert_eq!(action_calls.get(), 1, "action should be sent exactly once");
    assert_single_snapshot(&mock).await;
}

#[tokio::test]
async fn raw_prompt_waits_for_sse_open_before_dispatch() {
    run_action_is_gated(false).await;
}

#[tokio::test]
async fn command_waits_for_sse_open_before_dispatch() {
    run_action_is_gated(true).await;
}

async fn permission_action_is_gated(reply: PermissionReply) {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let reject = matches!(reply, PermissionReply::Reject);
    let session_id = if reject {
        "readiness-permission-reject"
    } else {
        "readiness-permission-allow"
    };
    let permission_id = format!("permission-{session_id}");

    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
                &permission_id,
                session_id,
                "file.write",
                &["src/**"],
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ]))
        .mount(&mock)
        .await;
    mount_empty_questions(&mock).await;
    let statuses = if reject {
        vec![ResponseTemplate::new(200).set_body_json(status_v2_idle())]
    } else {
        vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(session_id)),
            ResponseTemplate::new(200).set_body_json(status_v2_busy(session_id)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]
    };
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(statuses))
        .mount(&mock)
        .await;
    mount_messages(
        &mock,
        session_id,
        if reject {
            None
        } else {
            Some("PERMISSION_DONE")
        },
    )
    .await;
    let mut barrier = mount_barrier(&mock).await;

    let (action_responder, action_calls) =
        counted_response(ResponseTemplate::new(200).set_body_json(true));
    Mock::given(method("POST"))
        .and(path(format!("/permission/{permission_id}/reply")))
        .respond_with(action_responder)
        .mount(&mock)
        .await;

    let handle = tokio::spawn(async move {
        tool.call(
            RespondPermissionInput {
                session_id: session_id.into(),
                permission_request_id: Some(permission_id),
                reply,
                message: None,
            },
            &ToolContext::default(),
        )
        .await
    });

    barrier.wait_for_arrival().await;
    assert_eq!(action_calls.get(), 0, "permission reply preceded SSE Open");
    barrier.release();

    let output = timeout(TEST_TIMEOUT, handle)
        .await
        .expect("permission action did not complete")
        .expect("permission task panicked")
        .expect("permission action failed");
    assert!(matches!(output.status, RunStatus::Completed));
    if reject {
        assert!(output.response.is_none());
        assert!(
            output
                .warnings
                .iter()
                .any(|warning| warning.contains("rejected"))
        );
    } else {
        assert_eq!(output.response.as_deref(), Some("PERMISSION_DONE"));
        assert!(output.warnings.is_empty());
    }
    assert_eq!(action_calls.get(), 1, "permission action should occur once");
    assert_single_snapshot(&mock).await;
}

#[tokio::test]
async fn permission_allow_waits_for_sse_open_before_reply() {
    permission_action_is_gated(PermissionReply::Once).await;
}

#[tokio::test]
async fn permission_reject_waits_for_sse_open_before_reply() {
    permission_action_is_gated(PermissionReply::Reject).await;
}

async fn question_action_is_gated(action: QuestionAction) {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let reject = matches!(action, QuestionAction::Reject);
    let session_id = if reject {
        "readiness-question-reject"
    } else {
        "readiness-question-reply"
    };
    let question_id = format!("question-{session_id}");

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
                &question_id,
                session_id,
                &[question_payload("Continue?")],
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ]))
        .mount(&mock)
        .await;
    mount_empty_permissions(&mock).await;
    let statuses = if reject {
        vec![ResponseTemplate::new(200).set_body_json(status_v2_idle())]
    } else {
        vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(session_id)),
            ResponseTemplate::new(200).set_body_json(status_v2_busy(session_id)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]
    };
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(statuses))
        .mount(&mock)
        .await;
    mount_messages(
        &mock,
        session_id,
        if reject { None } else { Some("QUESTION_DONE") },
    )
    .await;
    let mut barrier = mount_barrier(&mock).await;

    let action_path = if reject {
        format!("/question/{question_id}/reject")
    } else {
        format!("/question/{question_id}/reply")
    };
    let (action_responder, action_calls) =
        counted_response(ResponseTemplate::new(200).set_body_json(true));
    Mock::given(method("POST"))
        .and(path(action_path))
        .respond_with(action_responder)
        .mount(&mock)
        .await;

    let handle = tokio::spawn(async move {
        tool.call(
            RespondQuestionInput {
                session_id: session_id.into(),
                question_request_id: Some(question_id),
                action,
                answers: if reject {
                    vec![]
                } else {
                    vec![vec!["yes".into()]]
                },
            },
            &ToolContext::default(),
        )
        .await
    });

    barrier.wait_for_arrival().await;
    assert_eq!(action_calls.get(), 0, "question action preceded SSE Open");
    barrier.release();

    let output = timeout(TEST_TIMEOUT, handle)
        .await
        .expect("question action did not complete")
        .expect("question task panicked")
        .expect("question action failed");
    assert!(matches!(output.status, RunStatus::Completed));
    assert_eq!(
        output.response.as_deref(),
        if reject { None } else { Some("QUESTION_DONE") }
    );
    assert!(output.warnings.is_empty());
    assert_eq!(action_calls.get(), 1, "question action should occur once");
    assert_single_snapshot(&mock).await;
}

#[tokio::test]
async fn question_reply_waits_for_sse_open_before_action() {
    question_action_is_gated(QuestionAction::Reply).await;
}

#[tokio::test]
async fn question_reject_waits_for_sse_open_before_action() {
    question_action_is_gated(QuestionAction::Reject).await;
}

#[tokio::test]
async fn readiness_timeout_sends_no_prompt() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server_with_config(
        &mock,
        OrchestratorConfig {
            sse_readiness_timeout_secs: 0,
            ..OrchestratorConfig::default()
        },
    )
    .await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let session_id = "readiness-timeout";
    mount_session(&mock, session_id).await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;
    mount_empty_permissions(&mock).await;
    mount_empty_questions(&mock).await;
    let _barrier = mount_barrier(&mock).await;
    let (action_responder, action_calls) = counted_response(ResponseTemplate::new(204));
    Mock::given(method("POST"))
        .and(path(format!("/session/{session_id}/prompt_async")))
        .respond_with(action_responder)
        .mount(&mock)
        .await;

    let error = timeout(
        TEST_TIMEOUT,
        tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.into()),
                command: None,
                agent: None,
                message: Some("must not dispatch".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("readiness timeout test hung")
    .expect_err("readiness timeout should fail");
    assert!(error.to_string().contains("no OpenCode action was sent"));
    assert_eq!(action_calls.get(), 0);
}

#[tokio::test]
async fn readiness_cancellation_sends_no_command_and_handle_remains_reusable() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let session_id = "readiness-cancel";
    mount_session(&mock, session_id).await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;
    mount_empty_permissions(&mock).await;
    mount_empty_questions(&mock).await;
    let mut barrier = mount_barrier(&mock).await;
    let (action_responder, action_calls) =
        counted_response(ResponseTemplate::new(200).set_body_json(serde_json::json!({})));
    Mock::given(method("POST"))
        .and(path(format!("/session/{session_id}/command")))
        .respond_with(action_responder)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(support::commands_list_fixture()))
        .mount(&mock)
        .await;

    let ctx = ToolContext::default();
    let cancellation = ctx.cancellation_token();
    let handle = tokio::spawn(async move {
        tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.into()),
                command: Some("research".into()),
                agent: None,
                message: Some("must not dispatch".into()),
                wait_for_activity: None,
            },
            &ctx,
        )
        .await
    });

    barrier.wait_for_arrival().await;
    assert_eq!(action_calls.get(), 0);
    cancellation.cancel();
    let result = timeout(TEST_TIMEOUT, handle)
        .await
        .expect("cancelled readiness did not finish")
        .expect("cancelled run task panicked");
    assert!(matches!(result, Err(ToolError::Cancelled { .. })));
    assert_eq!(action_calls.get(), 0);
    barrier.release();

    let commands = ListCommandsTool::new(Arc::clone(&server))
        .call(ListCommandsInput {}, &ToolContext::default())
        .await
        .expect("server handle should remain reusable");
    assert_eq!(commands.commands.len(), 3);
}

#[tokio::test]
async fn post_ready_permission_blocker_prevents_prompt_dispatch() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let session_id = "readiness-permission-blocker";
    mount_session(&mock, session_id).await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
                "post-ready-permission",
                session_id,
                "file.write",
                &["src/**"],
            )])),
        ]))
        .mount(&mock)
        .await;
    mount_empty_questions(&mock).await;
    let mut barrier = mount_barrier(&mock).await;
    let (action_responder, action_calls) = counted_response(ResponseTemplate::new(204));
    Mock::given(method("POST"))
        .and(path(format!("/session/{session_id}/prompt_async")))
        .respond_with(action_responder)
        .mount(&mock)
        .await;

    let handle = tokio::spawn(async move {
        tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.into()),
                command: None,
                agent: None,
                message: Some("blocked".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
    });
    barrier.wait_for_arrival().await;
    assert_eq!(action_calls.get(), 0);
    barrier.release();

    let output = timeout(TEST_TIMEOUT, handle)
        .await
        .expect("permission blocker test hung")
        .expect("permission blocker task panicked")
        .expect("permission blocker should return output");
    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(
        output.permission_request_id.as_deref(),
        Some("post-ready-permission")
    );
    assert_eq!(action_calls.get(), 0);
}

#[tokio::test]
async fn post_ready_question_blocker_prevents_command_dispatch() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let session_id = "readiness-question-blocker";
    mount_session(&mock, session_id).await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;
    mount_empty_permissions(&mock).await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
                "post-ready-question",
                session_id,
                &[question_payload("Continue?")],
            )])),
        ]))
        .mount(&mock)
        .await;
    let mut barrier = mount_barrier(&mock).await;
    let (action_responder, action_calls) =
        counted_response(ResponseTemplate::new(200).set_body_json(serde_json::json!({})));
    Mock::given(method("POST"))
        .and(path(format!("/session/{session_id}/command")))
        .respond_with(action_responder)
        .mount(&mock)
        .await;

    let handle = tokio::spawn(async move {
        tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.into()),
                command: Some("research".into()),
                agent: None,
                message: Some("blocked".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
    });
    barrier.wait_for_arrival().await;
    assert_eq!(action_calls.get(), 0);
    barrier.release();

    let output = timeout(TEST_TIMEOUT, handle)
        .await
        .expect("question blocker test hung")
        .expect("question blocker task panicked")
        .expect("question blocker should return output");
    assert!(matches!(output.status, RunStatus::QuestionRequired));
    assert_eq!(
        output.question_request_id.as_deref(),
        Some("post-ready-question")
    );
    assert_eq!(action_calls.get(), 0);
}
