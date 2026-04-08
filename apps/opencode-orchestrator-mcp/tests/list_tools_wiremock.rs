//! Wiremock tests for `list_sessions` and `list_commands` tools.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use opencode_orchestrator_mcp::tools::GetSessionStateTool;
use opencode_orchestrator_mcp::tools::ListCommandsTool;
use opencode_orchestrator_mcp::tools::ListSessionsTool;
use opencode_orchestrator_mcp::types::GetSessionStateInput;
use opencode_orchestrator_mcp::types::ListCommandsInput;
use opencode_orchestrator_mcp::types::ListSessionsInput;
use opencode_orchestrator_mcp::types::SessionStatusSummary;
use opencode_orchestrator_mcp::types::ToolStateSummary;
use serde_json::json;
use std::sync::Arc;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use support::busy_status_fixture;
use support::commands_list_fixture;
use support::message_fixture;
use support::message_history_fixture;
use support::retry_status_fixture;
use support::seed_spawned_sessions;
use support::session_fixture;
use support::session_status_fixture;
use support::sessions_list_fixture;
use support::test_orchestrator_server;
use support::tool_part_fixture;

#[tokio::test]
async fn list_sessions_returns_session_ids() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(sessions_list_fixture(&["ses-1", "ses-2"])),
        )
        .mount(&mock)
        .await;

    let tool = ListSessionsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListSessionsInput { limit: None }, &ToolContext::default())
        .await
        .expect("list_sessions should succeed");

    assert_eq!(result.sessions.len(), 2);
    assert_eq!(result.sessions[0].id, "ses-1");
    assert_eq!(result.sessions[1].id, "ses-2");
    assert!(result.sessions[0].status.is_none());
    assert!(result.sessions[1].status.is_none());
}

#[tokio::test]
async fn list_sessions_returns_empty_list() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    let tool = ListSessionsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListSessionsInput { limit: None }, &ToolContext::default())
        .await
        .expect("list_sessions should succeed");

    assert!(result.sessions.is_empty());
}

#[tokio::test]
async fn list_sessions_returns_enriched_fields() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "id": "ses-1",
                "slug": "ses-1",
                "projectId": "proj1",
                "directory": "/tmp/project-a",
                "title": "Session A",
                "version": "1.0",
                "summary": { "additions": 5, "deletions": 2, "files": 1 },
                "time": { "created": 10, "updated": 20 }
            },
            {
                "id": "ses-2",
                "slug": "ses-2",
                "projectId": "proj1",
                "directory": "/tmp/project-b",
                "title": "Session B",
                "version": "1.0",
                "summary": { "additions": 1, "deletions": 0, "files": 3 },
                "time": { "created": 30, "updated": 40 }
            }
        ])))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(session_status_fixture(&[
                ("ses-1", busy_status_fixture()),
                ("ses-2", retry_status_fixture(2, "rate limited", 1234)),
            ])),
        )
        .mount(&mock)
        .await;

    let tool = ListSessionsTool::new(Arc::clone(&server));
    let result = tool
        .call(ListSessionsInput { limit: None }, &ToolContext::default())
        .await
        .expect("list_sessions should succeed");

    assert_eq!(result.sessions.len(), 2);

    let first = &result.sessions[0];
    assert_eq!(first.created, Some(10));
    assert_eq!(first.updated, Some(20));
    assert_eq!(first.directory.as_deref(), Some("/tmp/project-a"));
    assert!(matches!(first.status, Some(SessionStatusSummary::Busy)));
    let first_stats = first.change_stats.as_ref().expect("change stats expected");
    assert_eq!(first_stats.additions, 5);
    assert_eq!(first_stats.deletions, 2);
    assert_eq!(first_stats.files, 1);

    let second = &result.sessions[1];
    assert!(matches!(
        second.status,
        Some(SessionStatusSummary::Retry {
            attempt: 2,
            ref message,
            next: 1234,
        }) if message == "rate limited"
    ));
}

#[tokio::test]
async fn list_sessions_marks_launched_by_you() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    seed_spawned_sessions(&server, &["ses-1"]).await;

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(sessions_list_fixture(&["ses-1", "ses-2"])),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&mock)
        .await;

    let tool = ListSessionsTool::new(Arc::clone(&server));
    let result = tool
        .call(ListSessionsInput { limit: None }, &ToolContext::default())
        .await
        .expect("list_sessions should succeed");

    assert!(result.sessions[0].launched_by_you);
    assert!(!result.sessions[1].launched_by_you);
}

#[tokio::test]
async fn get_session_state_returns_idle_status() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let result = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect("get_session_state should succeed");

    assert!(matches!(result.status, SessionStatusSummary::Idle));
    assert!(!result.launched_by_you);
}

#[tokio::test]
async fn get_session_state_returns_busy_status() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(session_status_fixture(&[("ses-1", busy_status_fixture())])),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let result = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect("get_session_state should succeed");

    assert!(matches!(result.status, SessionStatusSummary::Busy));
}

#[tokio::test]
async fn get_session_state_returns_retry_status() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(session_status_fixture(&[(
                "ses-1",
                retry_status_fixture(3, "provider overloaded", 9876),
            )])),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let result = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect("get_session_state should succeed");

    assert!(matches!(
        result.status,
        SessionStatusSummary::Retry {
            attempt: 3,
            ref message,
            next: 9876,
        } if message == "provider overloaded"
    ));
}

#[tokio::test]
async fn get_session_state_summarizes_messages_and_launched_by_you() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    seed_spawned_sessions(&server, &["ses-1"]).await;

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(session_status_fixture(&[("ses-1", busy_status_fixture())])),
        )
        .mount(&mock)
        .await;

    let history = message_history_fixture(vec![
        message_fixture(
            "ses-1",
            "m1",
            "assistant",
            1,
            Some(2),
            vec![tool_part_fixture(
                "call-pending",
                "read",
                Some(json!({ "status": "pending", "input": {}, "raw": "read" })),
            )],
        ),
        message_fixture(
            "ses-1",
            "m2",
            "assistant",
            2,
            Some(3),
            vec![tool_part_fixture(
                "call-running",
                "write",
                Some(json!({ "status": "running", "input": {}, "time": { "start": 20 } })),
            )],
        ),
        message_fixture(
            "ses-1",
            "m3",
            "assistant",
            3,
            Some(4),
            vec![tool_part_fixture(
                "call-completed",
                "grep",
                Some(json!({
                    "status": "completed",
                    "input": {},
                    "output": "done",
                    "title": "grep",
                    "metadata": {},
                    "time": { "start": 30, "end": 31 }
                })),
            )],
        ),
        message_fixture(
            "ses-1",
            "m4",
            "assistant",
            4,
            Some(5),
            vec![tool_part_fixture(
                "call-error",
                "edit",
                Some(json!({
                    "status": "error",
                    "input": {},
                    "error": "boom",
                    "time": { "start": 40, "end": 41 }
                })),
            )],
        ),
        message_fixture("ses-1", "m5", "user", 6, None, vec![]),
    ]);

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let result = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect("get_session_state should succeed");

    assert!(result.launched_by_you);
    assert_eq!(result.pending_message_count, 1);
    assert_eq!(result.last_activity, Some(6));
    assert_eq!(result.directory.as_deref(), Some("/tmp"));
    assert_eq!(result.recent_tool_calls.len(), 4);
    assert!(matches!(
        result.recent_tool_calls[0].state,
        ToolStateSummary::Error { ref message } if message == "boom"
    ));
    assert!(matches!(
        result.recent_tool_calls[1].state,
        ToolStateSummary::Completed
    ));
    assert!(matches!(
        result.recent_tool_calls[2].state,
        ToolStateSummary::Running
    ));
    assert!(matches!(
        result.recent_tool_calls[3].state,
        ToolStateSummary::Pending
    ));
}

#[tokio::test]
async fn get_session_state_returns_error_when_status_lookup_fails() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "name": "InternalError",
            "message": "boom"
        })))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let err = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("status lookup failure should error");

    assert!(matches!(err, ToolError::Internal(_)));
    assert!(err.to_string().contains("Failed to get session status"));
}

#[tokio::test]
async fn get_session_state_returns_error_when_message_lookup_fails() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(session_status_fixture(&[("ses-1", busy_status_fixture())])),
        )
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "name": "InternalError",
            "message": "boom"
        })))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let err = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("message lookup failure should error");

    assert!(matches!(err, ToolError::Internal(_)));
    assert!(err.to_string().contains("Failed to list messages"));
}

#[tokio::test]
async fn get_session_state_counts_each_trailing_user_message() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&mock)
        .await;

    let history = message_history_fixture(vec![
        message_fixture("ses-1", "m1", "assistant", 1, Some(2), vec![]),
        message_fixture("ses-1", "m2", "user", 3, None, vec![]),
        message_fixture("ses-1", "m3", "user", 4, None, vec![]),
    ]);

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let result = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect("get_session_state should succeed");

    assert_eq!(result.pending_message_count, 2);
}

#[tokio::test]
async fn get_session_state_unknown_session_returns_error() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "name": "NotFound",
            "message": "Session not found"
        })))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let err = tool
        .call(
            GetSessionStateInput {
                session_id: "missing".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("missing session should fail");

    assert!(matches!(err, ToolError::InvalidInput(_)));
    assert!(err.to_string().contains("Use list_sessions"));
}

/// When an LLM issues parallel tool calls, all of them appear as multiple `Part::Tool` entries
/// within a *single* assistant message. This test verifies that `get_session_state` returns all
/// tool calls from such a message and preserves their within-message ordering (first-to-last).
#[tokio::test]
async fn get_session_state_handles_parallel_tool_calls_in_single_message() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&mock)
        .await;

    // A single assistant message with THREE tool parts — this is what a parallel tool call
    // response looks like in the OpenCode message model.
    let history = message_history_fixture(vec![message_fixture(
        "ses-1",
        "m1",
        "assistant",
        100,
        Some(110),
        vec![
            tool_part_fixture(
                "call-grep",
                "grep",
                Some(json!({
                    "status": "completed",
                    "input": {},
                    "output": "results",
                    "title": "grep",
                    "metadata": {},
                    "time": { "start": 101, "end": 102 }
                })),
            ),
            tool_part_fixture(
                "call-read",
                "read",
                Some(json!({
                    "status": "completed",
                    "input": {},
                    "output": "content",
                    "title": "read",
                    "metadata": {},
                    "time": { "start": 103, "end": 104 }
                })),
            ),
            tool_part_fixture(
                "call-bash",
                "bash",
                Some(json!({
                    "status": "error",
                    "input": {},
                    "error": "command failed",
                    "time": { "start": 105, "end": 106 }
                })),
            ),
        ],
    )]);

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let result = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect("get_session_state should succeed with parallel tool calls");

    // All three parallel tool calls must be present.
    assert_eq!(result.recent_tool_calls.len(), 3);

    // Within-message ordering is preserved: calls come out in the order they appear in the
    // message (first → last), because the message itself is the unit of "recency".
    assert_eq!(result.recent_tool_calls[0].call_id, "call-grep");
    assert_eq!(result.recent_tool_calls[0].tool_name, "grep");
    assert!(matches!(
        result.recent_tool_calls[0].state,
        ToolStateSummary::Completed
    ));

    assert_eq!(result.recent_tool_calls[1].call_id, "call-read");
    assert_eq!(result.recent_tool_calls[1].tool_name, "read");
    assert!(matches!(
        result.recent_tool_calls[1].state,
        ToolStateSummary::Completed
    ));

    assert_eq!(result.recent_tool_calls[2].call_id, "call-bash");
    assert_eq!(result.recent_tool_calls[2].tool_name, "bash");
    assert!(matches!(
        result.recent_tool_calls[2].state,
        ToolStateSummary::Error { ref message } if message == "command failed"
    ));
}

/// The `recent_tool_calls` limit is applied globally across all parts of all messages.
/// When a single message contains more tool parts than the remaining budget, the batch is
/// truncated mid-message. This test documents that behaviour explicitly.
#[tokio::test]
async fn get_session_state_limit_truncates_within_parallel_batch() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session/ses-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("ses-1")))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&mock)
        .await;

    // Construct a history: an older message with 1 tool call, then a newer message with 3
    // parallel tool calls. With limit=10 (the default in GetSessionStateTool) all 4 fit, so
    // to force mid-batch truncation we build a history with enough calls to exceed the cap.
    // We use two messages, each with 6 parallel calls, and verify the limit of 10 is
    // respected and truncates the second (older) message's batch.
    let make_tool_parts = |prefix: &str| -> Vec<serde_json::Value> {
        (0..6)
            .map(|i| {
                tool_part_fixture(
                    &format!("{prefix}-{i}"),
                    "grep",
                    Some(json!({
                        "status": "completed",
                        "input": {},
                        "output": "ok",
                        "title": "grep",
                        "metadata": {},
                        "time": { "start": i, "end": i + 1 }
                    })),
                )
            })
            .collect()
    };

    let history = message_history_fixture(vec![
        // older message (processed second due to rev())
        message_fixture("ses-1", "m1", "assistant", 1, Some(2), make_tool_parts("old")),
        // newer message (processed first due to rev())
        message_fixture("ses-1", "m2", "assistant", 3, Some(4), make_tool_parts("new")),
    ]);

    Mock::given(method("GET"))
        .and(path("/session/ses-1/message"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history))
        .mount(&mock)
        .await;

    let tool = GetSessionStateTool::new(Arc::clone(&server));
    let result = tool
        .call(
            GetSessionStateInput {
                session_id: "ses-1".into(),
            },
            &ToolContext::default(),
        )
        .await
        .expect("get_session_state should succeed");

    // GetSessionStateTool uses a hard limit of 10.
    assert_eq!(result.recent_tool_calls.len(), 10);

    // The first 6 results come from the newer message ("new-0" … "new-5").
    for i in 0..6 {
        assert_eq!(result.recent_tool_calls[i].call_id, format!("new-{i}"));
    }
    // The next 4 come from the older message ("old-0" … "old-3"), truncated mid-batch.
    for i in 0..4 {
        assert_eq!(result.recent_tool_calls[6 + i].call_id, format!("old-{i}"));
    }
}

#[tokio::test]
async fn list_commands_returns_available_commands() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commands_list_fixture()))
        .mount(&mock)
        .await;

    let tool = ListCommandsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListCommandsInput {}, &ToolContext::default())
        .await
        .expect("list_commands should succeed");

    assert_eq!(result.commands.len(), 3);
    assert_eq!(result.commands[0].name, "test");
    assert_eq!(result.commands[1].name, "build");
    assert_eq!(result.commands[2].name, "lint");
}

#[tokio::test]
async fn list_commands_returns_empty_list() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    let tool = ListCommandsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListCommandsInput {}, &ToolContext::default())
        .await
        .expect("list_commands should succeed");

    assert!(result.commands.is_empty());
}