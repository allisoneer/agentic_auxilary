//! Wiremock coverage for orchestrator question interruption flows.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::tools::RespondQuestionTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use opencode_orchestrator_mcp::types::QuestionAction;
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
use wiremock::matchers::path_regex;

use support::SequenceResponder;
use support::messages_fixture;
use support::permission_fixture;
use support::question_fixture;
use support::session_fixture;
use support::status_v2_busy;
use support::status_v2_idle;
use support::test_orchestrator_server;

fn question_payload(question: &str) -> serde_json::Value {
    serde_json::json!({
        "question": question,
        "header": "Question Header",
        "options": [
            {"label": "yes", "description": "Proceed"},
            {"label": "no", "description": "Stop"}
        ],
        "multiple": false,
        "custom": false
    })
}

#[tokio::test]
async fn pending_question_preflight_returns_question_required() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "question-preflight";
    let question_id = "question-1";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture(question_id, sid, &[question_payload("Continue?")])
        ])))
        .mount(&mock)
        .await;

    let result = tool
        .call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: None,
                message: None,
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect("run should succeed");

    assert!(matches!(result.status, RunStatus::QuestionRequired));
    assert_eq!(result.question_request_id.as_deref(), Some(question_id));
    assert_eq!(result.questions.len(), 1);
    assert_eq!(result.questions[0].question, "Continue?");
}

#[tokio::test]
async fn poll_detected_question_returns_question_required() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "question-poll";
    let question_id = "question-2";

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

    let question_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
            question_id,
            sid,
            &[question_payload("Approve rollout?")]
        )])),
    ]);
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(question_seq)
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/session/{sid}/prompt_async")))
        .respond_with(ResponseTemplate::new(204))
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
        Duration::from_secs(3),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: None,
                message: Some("Do the work".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("run should not hang")
    .expect("run should succeed");

    assert!(matches!(result.status, RunStatus::QuestionRequired));
    assert_eq!(result.question_request_id.as_deref(), Some(question_id));
    assert_eq!(result.questions[0].question, "Approve rollout?");
}

#[tokio::test]
async fn respond_question_reply_resumes_to_completed() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let sid = "question-reply";
    let question_id = "question-3";

    let question_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
            question_id,
            sid,
            &[question_payload("Continue deployment?")]
        )])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(question_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
    ]);
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/question/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(sid, Some("QUESTION_REPLY_DONE"))),
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
        Duration::from_secs(4),
        tool.call(
            RespondQuestionInput {
                session_id: sid.into(),
                question_request_id: None,
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".into()]],
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("respond_question reply should not hang")
    .expect("respond_question reply should succeed");

    assert!(matches!(result.status, RunStatus::Completed));
    assert_eq!(result.response.as_deref(), Some("QUESTION_REPLY_DONE"));
}

#[tokio::test]
async fn respond_question_reject_completes_cleanly() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let sid = "question-reject";
    let question_id = "question-4";

    let question_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([question_fixture(
            question_id,
            sid,
            &[question_payload("Reject this?")]
        )])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(question_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
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

    Mock::given(method("POST"))
        .and(path_regex(r"/question/.*/reject"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(ResponseTemplate::new(200).set_body_json(messages_fixture(sid, None)))
        .mount(&mock)
        .await;

    let result = timeout(
        Duration::from_secs(2),
        tool.call(
            RespondQuestionInput {
                session_id: sid.into(),
                question_request_id: None,
                action: QuestionAction::Reject,
                answers: vec![],
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("respond_question reject should not hang")
    .expect("respond_question reject should succeed");

    assert!(matches!(result.status, RunStatus::Completed));
    assert!(result.response.is_none());
}

#[tokio::test]
async fn permission_priority_wins_over_question() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "question-priority";

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
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            permission_fixture("perm-priority", sid, "file.write", &["/tmp/out.txt"])
        ])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture("question-priority", sid, &[question_payload("Continue?")])
        ])))
        .mount(&mock)
        .await;

    let result = tool
        .call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: None,
                message: None,
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect("run should succeed");

    assert!(matches!(result.status, RunStatus::PermissionRequired));
    assert!(result.question_request_id.is_none());
}

// ==================== New Test Scenarios for v1.3.17 ====================

/// Test question lookup by explicit `question_request_id`
#[tokio::test]
async fn respond_question_by_id_lookup() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let sid = "question-by-id";
    let question_id = "question-specific";

    // Mount two questions for the same session, then empty after reply
    let question_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture(
                "question-other",
                sid,
                &[question_payload("First question?")]
            ),
            question_fixture(question_id, sid, &[question_payload("Second question?")])
        ])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(question_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    // Status transitions: busy -> busy -> idle
    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
    ]);
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    // Mock the reply endpoint to accept the specific question ID
    Mock::given(method("POST"))
        .and(path_regex(r"/question/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/session/{sid}/message")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("Reply done"))),
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

    // Provide explicit question_request_id to select the specific question
    let result = timeout(
        Duration::from_secs(4),
        tool.call(
            RespondQuestionInput {
                session_id: sid.into(),
                question_request_id: Some(question_id.into()),
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".into()]],
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("respond_question by id should not hang")
    .expect("respond_question by id should succeed");

    assert!(matches!(result.status, RunStatus::Completed));
}

/// Test error when multiple questions pending and no ID provided
#[tokio::test]
async fn multiple_pending_questions_returns_ambiguity_error() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let sid = "question-ambiguous";

    // Mount two questions for the same session
    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture("q1", sid, &[question_payload("First question?")]),
            question_fixture("q2", sid, &[question_payload("Second question?")])
        ])))
        .mount(&mock)
        .await;

    // Call without question_request_id - should return error
    let result = tool
        .call(
            RespondQuestionInput {
                session_id: sid.into(),
                question_request_id: None, // No explicit ID
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".into()]],
            },
            &ToolContext::default(),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Multiple pending questions"),
        "Expected ambiguity error, got: {msg}"
    );
    assert!(msg.contains("q1"), "Error should mention first question ID");
    assert!(
        msg.contains("q2"),
        "Error should mention second question ID"
    );
}

/// Test validation error when Reply action has empty answers
#[tokio::test]
async fn reply_with_empty_answers_returns_validation_error() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = RespondQuestionTool::new(Arc::clone(&server));
    let sid = "question-empty-answers";

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            question_fixture("q-empty", sid, &[question_payload("Continue?")])
        ])))
        .mount(&mock)
        .await;

    // Call with action=Reply but empty answers vec
    let result = tool
        .call(
            RespondQuestionInput {
                session_id: sid.into(),
                question_request_id: None,
                action: QuestionAction::Reply,
                answers: vec![], // Empty answers!
            },
            &ToolContext::default(),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("answers is required") || msg.contains("answers"),
        "Expected validation error about answers, got: {msg}"
    );
}
