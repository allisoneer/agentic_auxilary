#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::tools::RespondPermissionTool;
use opencode_orchestrator_mcp::tools::RespondQuestionTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use opencode_orchestrator_mcp::types::PermissionReply;
use opencode_orchestrator_mcp::types::QuestionAction;
use opencode_orchestrator_mcp::types::RespondPermissionInput;
use opencode_orchestrator_mcp::types::RespondQuestionInput;
use opencode_orchestrator_mcp::types::RunStatus;
use std::time::Duration;
use support::permission_fixture;
use support::question_fixture;
use support::readiness_server::ReadinessServer;
use support::readiness_server::ResponseAction;
use tokio::time::timeout;

const DEADLOCK_GUARD: Duration = Duration::from_secs(10);

fn permission(id: &str, session_id: &str) -> serde_json::Value {
    permission_fixture(id, session_id, "file.write", &["src/**"])
}

fn question(id: &str, session_id: &str) -> serde_json::Value {
    question_fixture(
        id,
        session_id,
        &[serde_json::json!({
            "question": "Continue?",
            "header": "Confirmation",
            "options": [{"label": "yes", "description": "Proceed"}],
            "multiple": false,
            "custom": false
        })],
    )
}

fn ok_list(items: Vec<serde_json::Value>) -> ResponseAction {
    ResponseAction::json(200, serde_json::Value::Array(items))
}

fn command_input(session_id: &str) -> OrchestratorRunInput {
    OrchestratorRunInput {
        session_id: Some(session_id.to_string()),
        command: Some("implement_plan".to_string()),
        agent: None,
        message: Some("do it".to_string()),
        wait_for_activity: None,
    }
}

fn prompt_input(session_id: &str) -> OrchestratorRunInput {
    OrchestratorRunInput {
        session_id: Some(session_id.to_string()),
        command: None,
        agent: None,
        message: Some("do it".to_string()),
        wait_for_activity: None,
    }
}

#[tokio::test]
async fn command_dispatch_waits_for_parsed_readiness() {
    let session_id = "command-readiness";
    let server = ReadinessServer::start(session_id).await;
    let tool = OrchestratorRunTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(command_input(session_id), &ToolContext::default())
            .await
    });

    server.wait_stream_connected().await;
    assert_eq!(server.command_count(), 0);
    server.release_readiness();
    server.wait_command().await;
    assert_eq!(server.command_count(), 1);
    run.abort();
    let _ = run.await;
}

#[tokio::test]
async fn prompt_dispatch_waits_for_parsed_readiness() {
    let session_id = "prompt-readiness";
    let server = ReadinessServer::start(session_id).await;
    let tool = OrchestratorRunTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(prompt_input(session_id), &ToolContext::default())
            .await
    });

    server.wait_stream_connected().await;
    assert_eq!(server.prompt_count(), 0);
    server.release_readiness();
    server.wait_prompt().await;
    assert_eq!(server.prompt_count(), 1);
    run.abort();
    let _ = run.await;
}

#[tokio::test]
async fn permission_reply_surfaces_immediate_second_blocker_from_same_stream() {
    let session_id = "permission-second";
    let first_id = "permission-first";
    let second_id = "permission-second";
    let server = ReadinessServer::start(session_id).await;
    server.push_permissions(ok_list(vec![permission(first_id, session_id)]));
    server.push_permissions(ok_list(vec![permission(first_id, session_id)]));
    server.push_permissions(ok_list(vec![]));
    let tool = RespondPermissionTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(
            RespondPermissionInput {
                session_id: session_id.to_string(),
                permission_request_id: Some(first_id.to_string()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        )
        .await
    });

    server.wait_stream_connected().await;
    assert_eq!(server.permission_response_count(), 0);
    server.release_readiness();
    server.wait_permission_response().await;
    server.append_event(serde_json::json!({
        "type": "permission.asked",
        "properties": permission(second_id, session_id)
    }));

    let output = timeout(DEADLOCK_GUARD, run)
        .await
        .expect("permission continuation should not deadlock")
        .expect("permission continuation task should not panic")
        .expect("permission continuation should succeed");
    assert!(matches!(output.status, RunStatus::PermissionRequired));
    assert_eq!(output.permission_request_id.as_deref(), Some(second_id));
    assert_eq!(server.permission_response_count(), 1);
}

#[tokio::test]
async fn question_reply_surfaces_immediate_second_blocker_from_same_stream() {
    let session_id = "question-second";
    let first_id = "question-first";
    let second_id = "question-second";
    let server = ReadinessServer::start(session_id).await;
    server.push_questions(ok_list(vec![question(first_id, session_id)]));
    server.push_questions(ok_list(vec![question(first_id, session_id)]));
    server.push_questions(ok_list(vec![]));
    let tool = RespondQuestionTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(
            RespondQuestionInput {
                session_id: session_id.to_string(),
                question_request_id: Some(first_id.to_string()),
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".to_string()]],
            },
            &ToolContext::default(),
        )
        .await
    });

    server.wait_stream_connected().await;
    assert_eq!(server.question_response_count(), 0);
    server.release_readiness();
    server.wait_question_response().await;
    server.append_event(serde_json::json!({
        "type": "question.asked",
        "properties": question(second_id, session_id)
    }));

    let output = timeout(DEADLOCK_GUARD, run)
        .await
        .expect("question continuation should not deadlock")
        .expect("question continuation task should not panic")
        .expect("question continuation should succeed");
    assert!(matches!(output.status, RunStatus::QuestionRequired));
    assert_eq!(output.question_request_id.as_deref(), Some(second_id));
    assert_eq!(server.question_response_count(), 1);
}

#[tokio::test]
async fn readiness_cancellation_prevents_dispatch() {
    let session_id = "cancel-readiness";
    let server = ReadinessServer::start(session_id).await;
    let tool = OrchestratorRunTool::new(server.orchestrator_server());
    let ctx = ToolContext::default();
    let cancellation = ctx.cancellation_token();
    let run = tokio::spawn(async move { tool.call(command_input(session_id), &ctx).await });

    server.wait_stream_connected().await;
    assert_eq!(server.command_count(), 0);
    cancellation.cancel();
    let error = timeout(DEADLOCK_GUARD, run)
        .await
        .expect("cancelled readiness should not deadlock")
        .expect("cancelled readiness task should not panic")
        .expect_err("cancelled readiness should fail");
    assert!(matches!(error, ToolError::Cancelled { .. }));
    assert_eq!(server.command_count(), 0);
}

#[tokio::test]
async fn readiness_timeout_prevents_dispatch() {
    let session_id = "timeout-readiness";
    let server = ReadinessServer::start(session_id).await;
    let tool = OrchestratorRunTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(command_input(session_id), &ToolContext::default())
            .await
    });

    server.wait_stream_connected().await;
    let error = timeout(Duration::from_secs(7), run)
        .await
        .expect("application readiness timeout should fire")
        .expect("readiness timeout task should not panic")
        .expect_err("readiness timeout should fail");
    assert!(matches!(error, ToolError::Internal(_)));
    assert!(error.to_string().contains("not ready within 5 seconds"));
    assert_eq!(server.command_count(), 0);
}

#[tokio::test]
async fn stream_closure_before_readiness_fails_closed() {
    let session_id = "closed-readiness";
    let server = ReadinessServer::start(session_id).await;
    let tool = OrchestratorRunTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(prompt_input(session_id), &ToolContext::default())
            .await
    });

    server.wait_stream_connected().await;
    server.close_stream_before_ready();
    let error = timeout(Duration::from_secs(7), run)
        .await
        .expect("closed-stream readiness should terminate")
        .expect("closed-stream task should not panic")
        .expect_err("closed stream must fail readiness");
    assert!(matches!(error, ToolError::Internal(_)));
    assert_eq!(server.prompt_count(), 0);
}

#[derive(Clone, Copy)]
enum PermissionRevalidationCase {
    BadRequest,
    Transport,
    Stale,
    Replacement,
    WrongSession,
}

async fn assert_permission_revalidation_fails_closed(case: PermissionRevalidationCase) {
    let session_id = "permission-revalidate";
    let request_id = "permission-target";
    let server = ReadinessServer::start(session_id).await;
    server.push_permissions(ok_list(vec![permission(request_id, session_id)]));
    server.push_permissions(match case {
        PermissionRevalidationCase::BadRequest => {
            ResponseAction::json(400, serde_json::json!({"message": "bad permission state"}))
        }
        PermissionRevalidationCase::Transport => ResponseAction::Close,
        PermissionRevalidationCase::Stale => ok_list(vec![]),
        PermissionRevalidationCase::Replacement => {
            ok_list(vec![permission("permission-replacement", session_id)])
        }
        PermissionRevalidationCase::WrongSession => {
            ok_list(vec![permission(request_id, "other-session")])
        }
    });
    let tool = RespondPermissionTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(
            RespondPermissionInput {
                session_id: session_id.to_string(),
                permission_request_id: Some(request_id.to_string()),
                reply: PermissionReply::Once,
                message: None,
            },
            &ToolContext::default(),
        )
        .await
    });
    server.wait_stream_connected().await;
    server.release_readiness();
    let error = timeout(DEADLOCK_GUARD, run)
        .await
        .expect("permission revalidation should not deadlock")
        .expect("permission revalidation task should not panic")
        .expect_err("permission revalidation should fail");
    match case {
        PermissionRevalidationCase::Transport => assert!(matches!(error, ToolError::Internal(_))),
        _ => assert!(matches!(error, ToolError::InvalidInput(_))),
    }
    assert_eq!(server.permission_response_count(), 0);
}

#[tokio::test]
async fn permission_revalidation_failures_never_send_a_reply() {
    for case in [
        PermissionRevalidationCase::BadRequest,
        PermissionRevalidationCase::Transport,
        PermissionRevalidationCase::Stale,
        PermissionRevalidationCase::Replacement,
        PermissionRevalidationCase::WrongSession,
    ] {
        assert_permission_revalidation_fails_closed(case).await;
    }
}

#[tokio::test]
async fn question_revalidation_replacement_never_sends_a_response() {
    let session_id = "question-revalidate";
    let request_id = "question-target";
    let server = ReadinessServer::start(session_id).await;
    server.push_questions(ok_list(vec![question(request_id, session_id)]));
    server.push_questions(ok_list(vec![question("question-replacement", session_id)]));
    let tool = RespondQuestionTool::new(server.orchestrator_server());
    let run = tokio::spawn(async move {
        tool.call(
            RespondQuestionInput {
                session_id: session_id.to_string(),
                question_request_id: Some(request_id.to_string()),
                action: QuestionAction::Reply,
                answers: vec![vec!["yes".to_string()]],
            },
            &ToolContext::default(),
        )
        .await
    });
    server.wait_stream_connected().await;
    server.release_readiness();
    let error = timeout(DEADLOCK_GUARD, run)
        .await
        .expect("question revalidation should not deadlock")
        .expect("question revalidation task should not panic")
        .expect_err("replacement question should fail revalidation");
    assert!(matches!(error, ToolError::InvalidInput(_)));
    assert!(error.to_string().contains("question-replacement"));
    assert_eq!(server.question_response_count(), 0);
}

async fn assert_question_404_is_invalid_input(action: QuestionAction) {
    let session_id = "question-404";
    let request_id = "question-target";
    let server = ReadinessServer::start(session_id).await;
    server.push_questions(ok_list(vec![question(request_id, session_id)]));
    server.push_questions(ok_list(vec![question(request_id, session_id)]));
    server.set_question_response_status(404);
    let tool = RespondQuestionTool::new(server.orchestrator_server());
    let answers = if matches!(action, QuestionAction::Reply) {
        vec![vec!["yes".to_string()]]
    } else {
        Vec::new()
    };
    let run = tokio::spawn(async move {
        tool.call(
            RespondQuestionInput {
                session_id: session_id.to_string(),
                question_request_id: Some(request_id.to_string()),
                action,
                answers,
            },
            &ToolContext::default(),
        )
        .await
    });
    server.wait_stream_connected().await;
    server.release_readiness();
    let error = timeout(DEADLOCK_GUARD, run)
        .await
        .expect("question 404 should not deadlock")
        .expect("question 404 task should not panic")
        .expect_err("question 404 should fail");
    assert!(matches!(error, ToolError::InvalidInput(_)));
    assert!(error.to_string().contains("HTTP 404"));
    assert_eq!(server.question_response_count(), 1);
}

#[tokio::test]
async fn question_reply_and_reject_404_are_actionable() {
    assert_question_404_is_invalid_input(QuestionAction::Reply).await;
    assert_question_404_is_invalid_input(QuestionAction::Reject).await;
}

#[tokio::test]
async fn permission_and_question_rejects_wait_for_readiness() {
    let permission_session = "permission-reject";
    let permission_id = "permission-target";
    let permission_server = ReadinessServer::start(permission_session).await;
    permission_server
        .push_permissions(ok_list(vec![permission(permission_id, permission_session)]));
    permission_server
        .push_permissions(ok_list(vec![permission(permission_id, permission_session)]));
    let permission_tool = RespondPermissionTool::new(permission_server.orchestrator_server());
    let permission_run = tokio::spawn(async move {
        permission_tool
            .call(
                RespondPermissionInput {
                    session_id: permission_session.to_string(),
                    permission_request_id: Some(permission_id.to_string()),
                    reply: PermissionReply::Reject,
                    message: None,
                },
                &ToolContext::default(),
            )
            .await
    });
    permission_server.wait_stream_connected().await;
    assert_eq!(permission_server.permission_response_count(), 0);
    permission_server.release_readiness();
    permission_server.wait_permission_response().await;
    permission_run.abort();
    let _ = permission_run.await;

    let question_session = "question-reject";
    let question_id = "question-target";
    let question_server = ReadinessServer::start(question_session).await;
    question_server.push_questions(ok_list(vec![question(question_id, question_session)]));
    question_server.push_questions(ok_list(vec![question(question_id, question_session)]));
    let question_tool = RespondQuestionTool::new(question_server.orchestrator_server());
    let question_run = tokio::spawn(async move {
        question_tool
            .call(
                RespondQuestionInput {
                    session_id: question_session.to_string(),
                    question_request_id: Some(question_id.to_string()),
                    action: QuestionAction::Reject,
                    answers: Vec::new(),
                },
                &ToolContext::default(),
            )
            .await
    });
    question_server.wait_stream_connected().await;
    assert_eq!(question_server.question_response_count(), 0);
    question_server.release_readiness();
    question_server.wait_question_response().await;
    question_run.abort();
    let _ = question_run.await;
}
