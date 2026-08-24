//! Wiremock coverage for caller-response blockers owned by descendant sessions.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::tools::RespondPermissionTool;
use opencode_orchestrator_mcp::tools::RespondQuestionTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use opencode_orchestrator_mcp::types::PermissionReply;
use opencode_orchestrator_mcp::types::QuestionAction;
use opencode_orchestrator_mcp::types::RespondPermissionInput;
use opencode_orchestrator_mcp::types::RespondQuestionInput;
use opencode_orchestrator_mcp::types::RunStatus;
use std::sync::Arc;
use std::time::Duration;
use support::SequenceResponder;
use support::messages_fixture;
use support::permission_fixture;
use support::question_fixture;
use support::session_fixture;
use support::session_fixture_with_parent;
use support::sse_body;
use support::status_v2_busy;
use support::status_v2_idle;
use support::test_orchestrator_server;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::path_regex;

fn question_payload(text: &str) -> serde_json::Value {
    serde_json::json!({
        "question": text,
        "header": "Decision",
        "options": [],
        "multiple": false,
        "custom": true,
    })
}

fn run_input(root_session_id: &str) -> OrchestratorRunInput {
    OrchestratorRunInput {
        session_id: Some(root_session_id.to_string()),
        command: None,
        agent: None,
        message: None,
        wait_for_activity: None,
    }
}

async fn mount_root_session(mock: &MockServer, root_session_id: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/session/{root_session_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(root_session_id)))
        .mount(mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(mock)
        .await;
}

async fn mount_descendant_session(
    mock: &MockServer,
    session_id: &str,
    parent_session_id: Option<&str>,
) {
    Mock::given(method("GET"))
        .and(path(format!("/session/{session_id}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(session_fixture_with_parent(session_id, parent_session_id)),
        )
        .mount(mock)
        .await;
}

async fn mount_hanging_sse(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(mock)
        .await;
}

#[tokio::test]
async fn preflight_detects_direct_child_permission() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-direct-permission";
    let child = "child-direct-permission";

    mount_root_session(&mock, root).await;
    mount_descendant_session(&mock, child, Some(root)).await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture("permission-child", child, "file.write", &["/tmp/out"])
        ])))
        .mount(&mock)
        .await;

    let output = tool
        .call(run_input(root), &ToolContext::default())
        .await
        .expect("descendant permission should be surfaced");

    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(
        output.permission_request_id.as_deref(),
        Some("permission-child")
    );
}

#[tokio::test]
async fn preflight_detects_deep_descendant_question() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-deep-question";
    let child = "child-deep-question";
    let grandchild = "grandchild-deep-question";

    mount_root_session(&mock, root).await;
    for (session_id, parent_id) in [(child, root), (grandchild, child)] {
        mount_descendant_session(&mock, session_id, Some(parent_id)).await;
    }
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture(
                "question-grandchild",
                grandchild,
                &[question_payload("Continue?")],
            )
        ])))
        .mount(&mock)
        .await;

    let output = tool
        .call(run_input(root), &ToolContext::default())
        .await
        .expect("deep descendant question should be surfaced");

    assert!(matches!(output.status, RunStatus::QuestionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(
        output.question_request_id.as_deref(),
        Some("question-grandchild")
    );
}

#[tokio::test]
async fn preflight_excludes_unrelated_cycle_and_broken_parent_chains() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-exclusions";

    mount_root_session(&mock, root).await;
    for (session_id, parent_id) in [
        ("unrelated", None),
        ("cycle-a", Some("cycle-b")),
        ("cycle-b", Some("cycle-a")),
        ("broken", Some("missing-parent")),
    ] {
        mount_descendant_session(&mock, session_id, parent_id).await;
    }
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture("permission-unrelated", "unrelated", "file.read", &["*"]),
            permission_fixture("permission-cycle", "cycle-a", "file.read", &["*"]),
            permission_fixture("permission-broken", "broken", "file.read", &["*"]),
        ])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{root}/message")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(root, Some("done"))),
        )
        .mount(&mock)
        .await;

    let output = timeout(
        Duration::from_secs(2),
        tool.call(run_input(root), &ToolContext::default()),
    )
    .await
    .expect("invalid chains must not hang")
    .expect("invalid chains should be ignored");

    assert!(matches!(output.status, RunStatus::Completed));
    assert_eq!(output.response.as_deref(), Some("done"));
}

#[tokio::test]
async fn selection_prefers_permission_then_depth_owner_and_request() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-ordering";

    mount_root_session(&mock, root).await;
    for child in ["owner-b", "owner-a"] {
        Mock::given(method("GET"))
            .and(path(format!("/session/{child}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(session_fixture_with_parent(child, Some(root))),
            )
            .mount(&mock)
            .await;
    }
    for (session_id, parent_id) in [("deep-parent", root), ("aaa-deep-owner", "deep-parent")] {
        Mock::given(method("GET"))
            .and(path(format!("/session/{session_id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(session_fixture_with_parent(session_id, Some(parent_id))),
            )
            .mount(&mock)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture("request-0", "aaa-deep-owner", "file.read", &["*"]),
            permission_fixture("request-z", "owner-a", "file.read", &["*"]),
            permission_fixture("request-a", "owner-b", "file.read", &["*"]),
            permission_fixture("request-a", "owner-a", "file.read", &["*"]),
        ])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture("root-question", root, &[question_payload("Root question?")])
        ])))
        .mount(&mock)
        .await;

    let output = tool
        .call(run_input(root), &ToolContext::default())
        .await
        .expect("deterministic blocker selection should succeed");

    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.permission_request_id.as_deref(), Some("request-a"));
}

#[tokio::test]
async fn polling_detects_descendant_permission_after_empty_preflight() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-polling";
    let child = "child-polling";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
                "permission-poll",
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
    mount_hanging_sse(&mock).await;

    let mut input = run_input(root);
    input.message = Some("start work".to_string());
    let output = timeout(
        Duration::from_secs(3),
        tool.call(input, &ToolContext::default()),
    )
    .await
    .expect("polling fallback should not hang")
    .expect("polling fallback should surface descendant blocker");

    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(
        output.permission_request_id.as_deref(),
        Some("permission-poll")
    );
}

#[tokio::test]
async fn polling_detects_descendant_question_after_empty_preflight() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-question-polling";
    let child = "child-question-polling";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
                "question-poll",
                child,
                &[question_payload("Continue polling?")],
            )])),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/session/{root}/prompt_async")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;
    mount_hanging_sse(&mock).await;

    let mut input = run_input(root);
    input.message = Some("start work".to_string());
    let output = timeout(
        Duration::from_secs(3),
        tool.call(input, &ToolContext::default()),
    )
    .await
    .expect("polling fallback should not hang")
    .expect("polling fallback should surface descendant question");

    assert!(matches!(output.status, RunStatus::QuestionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(output.question_request_id.as_deref(), Some("question-poll"));
}

#[tokio::test]
async fn sse_descendant_blocker_ignores_unrelated_ordinary_events() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-sse-isolation";
    let child = "child-sse-isolation";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
                "permission-sse-child",
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

    let events = [
        serde_json::json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "unrelated",
                "messageID": "unrelated-message",
                "delta": "must not leak",
            }
        }),
        serde_json::json!({
            "type": "session.idle",
            "properties": { "sessionID": "unrelated" }
        }),
        serde_json::json!({
            "type": "session.error",
            "properties": {
                "sessionID": "unrelated",
                "error": { "message": "must be ignored" }
            }
        }),
        serde_json::json!({
            "type": "permission.asked",
            "properties": permission_fixture(
                "permission-sse-child",
                child,
                "bash",
                &["*"],
            )
        }),
    ];
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body(&events), "text/event-stream"),
        )
        .mount(&mock)
        .await;

    let mut input = run_input(root);
    input.message = Some("start work".to_string());
    let output = timeout(
        Duration::from_secs(3),
        tool.call(input, &ToolContext::default()),
    )
    .await
    .expect("descendant SSE blocker should not hang")
    .expect("unrelated ordinary events should be ignored");

    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(
        output.permission_request_id.as_deref(),
        Some("permission-sse-child")
    );
    assert!(output.partial_response.is_none());
}

#[tokio::test]
async fn sse_descendant_question_triggers_authoritative_scan() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-sse-question";
    let child = "child-sse-question";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
                "question-sse-child",
                child,
                &[question_payload("Continue from child?")],
            )])),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/session/{root}/prompt_async")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;
    let interruption = serde_json::json!({
        "type": "question.asked",
        "properties": question_fixture(
            "question-sse-child",
            child,
            &[question_payload("Continue from child?")],
        )
    });
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body(&[interruption]), "text/event-stream"),
        )
        .mount(&mock)
        .await;

    let mut input = run_input(root);
    input.message = Some("start work".to_string());
    let output = timeout(
        Duration::from_secs(3),
        tool.call(input, &ToolContext::default()),
    )
    .await
    .expect("descendant question SSE should not hang")
    .expect("descendant question should be surfaced");

    assert!(matches!(output.status, RunStatus::QuestionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(
        output.question_request_id.as_deref(),
        Some("question-sse-child")
    );
}

#[tokio::test]
async fn sse_before_list_persistence_is_recovered_by_later_scan() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-sse-race";
    let child = "child-sse-race";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy(root)))
        .mount(&mock)
        .await;
    let permission_sequence = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
            "permission-after-race",
            child,
            "bash",
            &["*"],
        )])),
    ]);
    let permission_calls = permission_sequence.call_counter();
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(permission_sequence)
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
    let interruption = serde_json::json!({
        "type": "permission.asked",
        "properties": permission_fixture("permission-after-race", child, "bash", &["*"])
    });
    let root_idle = serde_json::json!({
        "type": "session.idle",
        "properties": { "sessionID": root }
    });
    let root_activity = serde_json::json!({
        "type": "message.updated",
        "properties": {
            "info": {
                "id": "root-before-descendant-asked",
                "sessionID": root,
                "role": "assistant",
                "time": { "created": 1 }
            }
        }
    });
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse_body(&[root_activity, root_idle, interruption]),
            "text/event-stream",
        ))
        .mount(&mock)
        .await;

    let mut input = run_input(root);
    input.message = Some("start work".to_string());
    let output = timeout(
        Duration::from_secs(3),
        tool.call(input, &ToolContext::default()),
    )
    .await
    .expect("persistence race recovery should not hang")
    .expect("a later scan should surface the blocker");

    assert!(permission_calls.get() >= 4);
    assert_eq!(
        output.permission_request_id.as_deref(),
        Some("permission-after-race")
    );
}

#[tokio::test]
async fn transient_asked_event_ancestry_failure_stays_latched_until_blocker_is_visible() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-transient-event-ancestry";
    let child = "child-transient-event-ancestry";
    let request_id = "permission-transient-event-ancestry";

    Mock::given(method("GET"))
        .and(path(format!("/session/{root}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{child}")))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(503).set_body_string("transient session lookup failure"),
            ResponseTemplate::new(200)
                .set_body_json(session_fixture_with_parent(child, Some(root))),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(root)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
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
                .set_body_json(messages_fixture(root, Some("MUST_NOT_COMPLETE"))),
        )
        .mount(&mock)
        .await;
    let events = [
        serde_json::json!({
            "type": "permission.asked",
            "properties": permission_fixture(request_id, child, "bash", &["*"])
        }),
        serde_json::json!({
            "type": "message.updated",
            "properties": {
                "info": {
                    "id": "root-after-transient-ancestry",
                    "sessionID": root,
                    "role": "assistant",
                    "time": { "created": 1 }
                }
            }
        }),
        serde_json::json!({
            "type": "session.idle",
            "properties": { "sessionID": root }
        }),
    ];
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(50))
                .set_body_raw(sse_body(&events), "text/event-stream"),
        )
        .mount(&mock)
        .await;

    let mut input = run_input(root);
    input.message = Some("start work".to_string());
    let output = timeout(
        Duration::from_secs(5),
        tool.call(input, &ToolContext::default()),
    )
    .await
    .expect("transient event ancestry failure should not hang")
    .expect("later authoritative polling should surface the blocker");

    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(output.permission_request_id.as_deref(), Some(request_id));
    assert!(output.response.is_none());
}

#[tokio::test]
async fn unrelated_caller_response_event_does_not_delay_root_completion() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-unrelated-event-control";
    let unrelated = "unrelated-event-owner";

    Mock::given(method("GET"))
        .and(path(format!("/session/{root}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{unrelated}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(unrelated)))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .expect(0)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(root)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;
    let permission_responder = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    let permission_calls = permission_responder.call_counter();
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(permission_responder)
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
            ResponseTemplate::new(200).set_body_json(messages_fixture(root, Some("ROOT_DONE"))),
        )
        .mount(&mock)
        .await;
    let events = [
        serde_json::json!({
            "type": "permission.asked",
            "properties": permission_fixture(
                "permission-unrelated-event",
                unrelated,
                "bash",
                &["*"],
            )
        }),
        serde_json::json!({
            "type": "session.idle",
            "properties": { "sessionID": root }
        }),
    ];
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body(&events), "text/event-stream"),
        )
        .mount(&mock)
        .await;

    let output = timeout(
        Duration::from_secs(3),
        tool.call(run_input(root), &ToolContext::default()),
    )
    .await
    .expect("unrelated event control should not hang")
    .expect("unrelated event must not block completion");

    assert!(matches!(output.status, RunStatus::Completed));
    assert_eq!(output.response.as_deref(), Some("ROOT_DONE"));
    assert!(permission_calls.get() >= 2);
}

#[tokio::test]
async fn stale_eligible_event_latch_clears_after_two_authoritative_polls() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-stale-event";
    let child = "child-stale-event";

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
            ResponseTemplate::new(200).set_body_json(status_v2_busy(root)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;
    let permission_responder = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    let permission_calls = permission_responder.call_counter();
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(permission_responder)
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
                .set_body_json(messages_fixture(root, Some("STALE_EVENT_CLEARED"))),
        )
        .mount(&mock)
        .await;
    let events = [
        serde_json::json!({
            "type": "permission.asked",
            "properties": permission_fixture(
                "permission-stale-event",
                child,
                "bash",
                &["*"],
            )
        }),
        serde_json::json!({
            "type": "message.updated",
            "properties": {
                "info": {
                    "id": "root-stale-activity",
                    "sessionID": root,
                    "role": "assistant",
                    "time": { "created": 1 }
                }
            }
        }),
        serde_json::json!({
            "type": "session.idle",
            "properties": { "sessionID": root }
        }),
    ];
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_raw(sse_body(&events), "text/event-stream"),
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        ]))
        .mount(&mock)
        .await;

    let mut input = run_input(root);
    input.message = Some("start work".to_string());
    let output = timeout(
        Duration::from_secs(5),
        tool.call(input, &ToolContext::default()),
    )
    .await
    .expect("stale event latch should clear boundedly")
    .expect("root should complete after two authoritative clear polls");

    assert!(matches!(output.status, RunStatus::Completed));
    assert_eq!(output.response.as_deref(), Some("STALE_EVENT_CLEARED"));
    assert!(permission_calls.get() >= 5);
}

#[tokio::test]
async fn respond_permission_accepts_descendant_explicit_id_and_resumes_root() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let root = "root-permission-reply";
    let child = "child-permission-reply";
    let request_id = "permission-descendant-reply";

    Mock::given(method("GET"))
        .and(path(format!("/session/{child}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(session_fixture_with_parent(child, Some(root))),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{root}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
                request_id,
                child,
                "file.write",
                &["/tmp/out"]
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/permission/{request_id}/reply")))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(root)),
            ResponseTemplate::new(200).set_body_json(status_v2_busy(root)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{root}/message")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(root, Some("ROOT_PERMISSION_DONE"))),
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
        Duration::from_secs(4),
        tool.call(
            RespondPermissionInput {
                session_id: root.to_string(),
                permission_request_id: Some(request_id.to_string()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("descendant permission reply should not hang")
    .expect("descendant permission reply should succeed");

    assert!(matches!(output.status, RunStatus::Completed));
    assert_eq!(output.session_id, root);
    assert_eq!(output.response.as_deref(), Some("ROOT_PERMISSION_DONE"));
}

#[tokio::test]
async fn respond_permission_rejects_unrelated_explicit_id() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let root = "root-permission-unrelated";
    let unrelated = "unrelated-permission-owner";
    let request_id = "permission-unrelated";

    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture(request_id, unrelated, "file.read", &["*"])
        ])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{unrelated}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(unrelated)))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .expect(0)
        .mount(&mock)
        .await;

    let error = tool
        .call(
            RespondPermissionInput {
                session_id: root.to_string(),
                permission_request_id: Some(request_id.to_string()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("unrelated permission owner must be rejected");

    assert!(error.to_string().contains("not session"));
}

#[tokio::test]
async fn respond_permission_fails_closed_on_explicit_id_ancestry_error() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondPermissionTool::new(Arc::clone(&server));
    let root = "root-permission-ancestry-error";
    let child = "child-permission-ancestry-error";
    let request_id = "permission-ancestry-error";

    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture(request_id, child, "file.read", &["*"])
        ])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{child}")))
        .respond_with(ResponseTemplate::new(503).set_body_string("session lookup unavailable"))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .expect(0)
        .mount(&mock)
        .await;

    let error = tool
        .call(
            RespondPermissionInput {
                session_id: root.to_string(),
                permission_request_id: Some(request_id.to_string()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("ancestry lookup failure must prevent the permission reply");

    assert!(
        error
            .to_string()
            .contains("Failed to verify permission request")
    );
    assert!(error.to_string().contains("503"));
}

#[tokio::test]
async fn respond_question_accepts_descendant_explicit_id_and_resumes_root() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let root = "root-question-reply";
    let child = "child-question-reply";
    let request_id = "question-descendant-reply";

    Mock::given(method("GET"))
        .and(path(format!("/session/{child}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(session_fixture_with_parent(child, Some(root))),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{root}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(root)))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
                request_id,
                child,
                &[question_payload("Continue?")],
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/question/{request_id}/reply")))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(status_v2_busy(root)),
            ResponseTemplate::new(200).set_body_json(status_v2_busy(root)),
            ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ]))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{root}/message")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(root, Some("ROOT_QUESTION_DONE"))),
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
        Duration::from_secs(4),
        tool.call(
            RespondQuestionInput {
                session_id: root.to_string(),
                question_request_id: Some(request_id.to_string()),
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".to_string()]],
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("descendant question reply should not hang")
    .expect("descendant question reply should succeed");

    assert!(matches!(output.status, RunStatus::Completed));
    assert_eq!(output.session_id, root);
    assert_eq!(output.response.as_deref(), Some("ROOT_QUESTION_DONE"));
}

#[tokio::test]
async fn respond_question_rejects_unrelated_explicit_id() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let root = "root-question-unrelated";
    let unrelated = "unrelated-question-owner";
    let request_id = "question-unrelated";

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture(request_id, unrelated, &[question_payload("Continue?")])
        ])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/session/{unrelated}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(unrelated)))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/question/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .expect(0)
        .mount(&mock)
        .await;

    let error = tool
        .call(
            RespondQuestionInput {
                session_id: root.to_string(),
                question_request_id: Some(request_id.to_string()),
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".to_string()]],
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("unrelated question owner must be rejected");

    assert!(error.to_string().contains("not session"));
}

#[tokio::test]
async fn respond_question_list_failure_has_no_compatibility_bypass() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = RespondQuestionTool::new(Arc::clone(&server));

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "invalid question state"
        })))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/question/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .expect(0)
        .mount(&mock)
        .await;

    let error = tool
        .call(
            RespondQuestionInput {
                session_id: "root-question-list-error".to_string(),
                question_request_id: Some("question-list-error".to_string()),
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".to_string()]],
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("question list failure must not bypass ownership validation");

    assert!(error.to_string().contains("Failed to list questions"));
}

#[tokio::test]
async fn polling_final_confirmation_surfaces_new_descendant_blocker() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let root = "root-resume-idle-confirmation";
    let child = "child-resume-idle-confirmation";
    let request_id = "permission-resume-idle-confirmation";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
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
    Mock::given(method("GET"))
        .and(path(format!("/session/{root}/message")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(root, Some("MUST_NOT_COMPLETE"))),
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
        Duration::from_secs(3),
        tool.call(run_input(root), &ToolContext::default()),
    )
    .await
    .expect("resume-only confirmation should not hang")
    .expect("resume-only confirmation should surface the blocker");

    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.session_id, root);
    assert_eq!(output.permission_request_id.as_deref(), Some(request_id));
    assert!(output.response.is_none());
}
