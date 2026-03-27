//! Wiremock-based integration tests for permission response flow bugs.
//!
//! These tests deterministically reproduce four user-visible bugs:
//! - IT-BUG1: Empty responses after permission completion (message extraction race)
//! - IT-BUG2: Misleading response on rejection (returns stale pre-rejection text)
//! - IT-BUG3: Session requires resumption to get response (same race as BUG1)
//! - IT-BUG4: Network error on command dispatch (no bounded HTTP retry)
//!
//! The tests are designed to FAIL on current code (pre-fix), confirming the bugs exist.
//! After implementing fixes, all tests should PASS.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use agentic_tools_core::{Tool, ToolContext};
use opencode_orchestrator_mcp::tools::{OrchestratorRunTool, RespondPermissionTool};
use opencode_orchestrator_mcp::types::{
    OrchestratorRunInput, PermissionReply, RespondPermissionInput, RunStatus,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::{
    SequenceResponder, SwitchAfterCallsResponder, messages_fixture, permission_fixture,
    session_fixture, status_v2_busy, status_v2_idle, status_v2_retry, test_orchestrator_server,
};

/// IT-BUG1: Completion should retry message extraction when first attempt returns no assistant text.
///
/// Pre-fix behavior: Single `messages.list` call returns None -> response is empty.
/// Post-fix behavior: Retry with backoff until assistant text appears.
#[tokio::test]
async fn it_bug1_completion_retries_messages_until_visible() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let sid = "s1";

    // GET /session/s1 - session exists
    Mock::given(method("GET"))
        .and(path("/session/s1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    // GET /session/status - busy initially (multiple times for initial check + polling), then idle
    // First call: initial status check before SSE subscription
    // Second call: poll interval check (observed_busy=true since our session is busy)
    // Third+ calls: idle (triggers finalize_completed)
    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)), // initial check
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)), // poll: sets observed_busy=true
        ResponseTemplate::new(200).set_body_json(status_v2_idle()), // poll: idle, triggers completion
    ]);
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    // GET /permission - no pending permissions
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

    // GET /session/s1/message - FIRST call: no assistant text, SECOND call: has text
    let messages_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(messages_fixture(sid, None)), // stale
        ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("FINAL_RESPONSE"))), // fresh
    ]);
    let messages_call_counter = messages_seq.call_counter();
    Mock::given(method("GET"))
        .and(path("/session/s1/message"))
        .respond_with(messages_seq)
        .mount(&mock)
        .await;

    // POST /session/s1/prompt_async - fire and forget
    Mock::given(method("POST"))
        .and(path("/session/s1/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    // GET /event - SSE endpoint: return empty response with delay to allow polling to complete
    // The SSE subscription will fail/hang, but polling will detect idle status
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)), // Long delay to let polling win
        )
        .mount(&mock)
        .await;

    // Act
    let result = timeout(
        Duration::from_secs(10),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: None,
                message: Some("test prompt".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("timed out")
    .expect("tool error");

    // Assert
    assert!(
        matches!(result.status, RunStatus::Completed),
        "expected Completed status, got {:?}",
        result.status
    );
    assert_eq!(
        result.response.as_deref(),
        Some("FINAL_RESPONSE"),
        "Pre-fix: response is None (single message fetch); Post-fix: response is Some after retry"
    );
    // Pre-fix: call_count == 1, response is None
    // Post-fix: call_count >= 2, response is Some
    assert!(
        messages_call_counter.get() >= 2,
        "expected retry; got {} calls to messages.list",
        messages_call_counter.get()
    );
}

/// IT-BUG2: Rejection should return response=None with warning, NOT stale pre-rejection text.
///
/// Pre-fix behavior: Returns `I_WILL_CREATE_FILE` (stale text from before permission request).
/// Post-fix behavior: Returns response=None with warning about rejection.
#[tokio::test]
async fn it_bug2_reject_returns_none_and_warning_not_stale_text() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let respond_tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "s2";
    let perm_id = "perm-123";

    // GET /session/s2
    Mock::given(method("GET"))
        .and(path("/session/s2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    // GET /session/status - idle after rejection
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;

    // GET /permission - has pending permission before reply, empty after
    let perm_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
            perm_id,
            sid,
            "file.write",
            &["/tmp/test.txt"]
        )])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(perm_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    // POST /permission/{id}/reply - accept the rejection
    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    // GET /session/s2/message - returns STALE pre-rejection text (baseline and final same)
    Mock::given(method("GET"))
        .and(path("/session/s2/message"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(messages_fixture(sid, Some("I_WILL_CREATE_FILE"))),
        )
        .mount(&mock)
        .await;

    // Act
    let result = timeout(
        Duration::from_secs(10),
        respond_tool.call(
            RespondPermissionInput {
                session_id: sid.into(),
                permission_request_id: None,
                reply: PermissionReply::Reject,
                message: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("timed out")
    .expect("tool error");

    // Assert
    assert!(
        matches!(result.status, RunStatus::Completed),
        "expected Completed status, got {:?}",
        result.status
    );
    // Pre-fix: response == Some("I_WILL_CREATE_FILE"), warnings empty
    // Post-fix: response == None, warnings contains "Permission rejected"
    assert!(
        result.response.is_none(),
        "expected response=None after rejection, got {:?}",
        result.response
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.to_lowercase().contains("reject")),
        "expected warning about rejection, got {:?}",
        result.warnings
    );
}

/// IT-BUG3: `respond_permission` should return final response in same call (no resumption needed).
///
/// Pre-fix behavior: Returns Completed but response=None; need separate resume call.
/// Post-fix behavior: Returns Completed with response=Some in single call.
#[tokio::test]
async fn it_bug3_respond_permission_returns_response_without_resumption() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let respond_tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "s3";
    let perm_id = "perm-456";

    // GET /session/s3
    Mock::given(method("GET"))
        .and(path("/session/s3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    // GET /session/status - starts idle (pre-fix early-exit #1), then busy, then idle.
    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_idle()), // initial check: idle
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)), // later: busy
        ResponseTemplate::new(200).set_body_json(status_v2_idle()), // later: idle -> completion
    ]);
    let status_calls = status_seq.call_counter();
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    // GET /permission - has pending permission before reply
    let perm_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!([permission_fixture(
            perm_id,
            sid,
            "file.write",
            &["/tmp/out.txt"]
        )])),
        ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
    ]);
    Mock::given(method("GET"))
        .and(path("/permission"))
        .respond_with(perm_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    // POST /permission/{id}/reply
    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    // Message endpoint: if we finalize after only one status call, no assistant message yet.
    // After additional status polls (fixed path), return final assistant text.
    let msg_before = ResponseTemplate::new(200).set_body_json(messages_fixture(sid, None));
    let msg_after = ResponseTemplate::new(200)
        .set_body_json(messages_fixture(sid, Some("PERMISSION_GRANTED_RESPONSE")));
    Mock::given(method("GET"))
        .and(path("/session/s3/message"))
        .respond_with(SwitchAfterCallsResponder::new(
            status_calls.clone(),
            2,
            msg_before,
            msg_after,
        ))
        .mount(&mock)
        .await;

    // Act
    let result = timeout(
        Duration::from_secs(10),
        respond_tool.call(
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
    .expect("timed out")
    .expect("tool error");

    // Assert
    assert!(
        matches!(result.status, RunStatus::Completed),
        "expected Completed status, got {:?}",
        result.status
    );
    // Pre-fix: response == None (need extra resume call)
    // Post-fix: response == Some (single call completes with response)
    assert_eq!(
        result.response.as_deref(),
        Some("PERMISSION_GRANTED_RESPONSE"),
        "Pre-fix: response is None (early exit); Post-fix: response is Some after waiting"
    );
}

/// IT-BUG5: `respond_permission` should not return stale pre-permission text.
///
/// Pre-fix behavior: Returns `PRE_PERMISSION_TEXT` due to post-subscribe early-exit (#2).
/// Post-fix behavior: Waits for post-permission activity and returns `POST_PERMISSION_TEXT`.
#[tokio::test]
async fn it_bug5_respond_permission_waits_and_does_not_return_stale_pre_permission_text() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);
    let respond_tool = RespondPermissionTool::new(Arc::clone(&server));
    let sid = "s5";
    let perm_id = "perm-999";

    // GET /session/s5
    Mock::given(method("GET"))
        .and(path("/session/s5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    // GET /permission - pending permission before reply, then empty after
    let perm_seq = SequenceResponder::new(vec![
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
        .respond_with(perm_seq)
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/question"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    // POST /permission/{id}/reply
    Mock::given(method("POST"))
        .and(path_regex(r"/permission/.*/reply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(true))
        .mount(&mock)
        .await;

    // GET /session/status
    // 1) busy  (avoid early-exit #1)
    // 2) idle  (pre-fix early-exit #2)
    // 3) retry (fixed path observes activity)
    // 4) idle  (fixed path finalizes)
    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)),
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ResponseTemplate::new(200).set_body_json(status_v2_retry(sid, 1)),
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
    ]);
    let status_calls = status_seq.call_counter();
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    // GET /session/s5/message
    // Before enough status calls, return stale pre-permission text.
    // After waiting, return post-permission text.
    let msg_pre = ResponseTemplate::new(200)
        .set_body_json(messages_fixture(sid, Some("PRE_PERMISSION_TEXT")));
    let msg_post = ResponseTemplate::new(200)
        .set_body_json(messages_fixture(sid, Some("POST_PERMISSION_TEXT")));
    Mock::given(method("GET"))
        .and(path("/session/s5/message"))
        .respond_with(SwitchAfterCallsResponder::new(
            status_calls.clone(),
            3,
            msg_pre,
            msg_post,
        ))
        .mount(&mock)
        .await;

    // GET /event - delay SSE so polling drives completion
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;

    // Act
    let result = timeout(
        Duration::from_secs(10),
        respond_tool.call(
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
    .expect("timed out")
    .expect("tool error");

    // Assert
    assert!(
        matches!(result.status, RunStatus::Completed),
        "expected Completed status, got {:?}",
        result.status
    );
    assert_eq!(result.response.as_deref(), Some("POST_PERMISSION_TEXT"));
}

/// IT-BUG4: Command dispatch should retry on transport-level timeout.
///
/// Pre-fix behavior: First timeout error propagates immediately.
/// Post-fix behavior: Retry succeeds, command executes.
#[tokio::test]
async fn it_bug4_command_dispatch_retries_on_transport_error() {
    let mock = MockServer::start().await;
    // Use short timeout client (1 second)
    let base_url = mock.uri().trim_end_matches('/').to_string();
    let client = opencode_rs::ClientBuilder::new()
        .base_url(&base_url)
        .timeout_secs(1) // 1 second timeout
        .build()
        .unwrap();
    let server_cell = Arc::new(tokio::sync::OnceCell::new());
    server_cell
        .set(
            opencode_orchestrator_mcp::server::OrchestratorServer::from_client_unshared(
                client, &base_url,
            ),
        )
        .unwrap_or_else(|_| panic!("cell should be empty"));
    let tool = OrchestratorRunTool::new(Arc::clone(&server_cell));
    let sid = "s4";

    // GET /session/s4
    Mock::given(method("GET"))
        .and(path("/session/s4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture(sid)))
        .mount(&mock)
        .await;

    // GET /session/status - busy initially, then idle (after command succeeds)
    // Multiple busy responses to handle initial check + polling
    let status_seq = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)), // initial check
        ResponseTemplate::new(200).set_body_json(status_v2_busy(sid)), // poll: sets observed_busy=true
        ResponseTemplate::new(200).set_body_json(status_v2_idle()), // poll: idle, triggers completion
    ]);
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_seq)
        .mount(&mock)
        .await;

    // GET /permission
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

    // GET /session/s4/message
    Mock::given(method("GET"))
        .and(path("/session/s4/message"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(messages_fixture(sid, Some("COMMAND_RESULT"))),
        )
        .mount(&mock)
        .await;

    // GET /event - SSE endpoint: return empty response with delay to allow polling to complete
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)), // Long delay to let polling win
        )
        .mount(&mock)
        .await;

    // POST /session/s4/command - FIRST: timeout (delay > client timeout), SECOND: success
    // Note: wiremock-rs doesn't have Fault types, so we use set_delay to cause timeout

    // Mount first request with long delay (causes timeout)
    Mock::given(method("POST"))
        .and(path("/session/s4/command"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "executed"}))
                .set_delay(Duration::from_secs(30)), // 30s delay > 1s timeout
        )
        .up_to_n_times(1) // Only first request
        .mount(&mock)
        .await;

    // Mount second request with immediate response
    Mock::given(method("POST"))
        .and(path("/session/s4/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "executed"})),
        )
        .mount(&mock)
        .await;

    // Act
    let result = timeout(
        Duration::from_secs(15), // Overall test timeout
        tool.call(
            OrchestratorRunInput {
                session_id: Some(sid.into()),
                command: Some("test_cmd".into()),
                message: Some("test args".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("test timed out");

    // Assert
    // Pre-fix: Error returned due to timeout on first command attempt
    // Post-fix: Retry succeeds, status is Completed
    assert!(
        result.is_ok(),
        "Pre-fix: command times out; Post-fix: retry succeeds. Got error: {:?}",
        result.err()
    );

    let output = result.unwrap();
    assert!(
        matches!(output.status, RunStatus::Completed),
        "expected Completed status after retry"
    );
}
