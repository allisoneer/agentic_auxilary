//! Wiremock-based integration tests for orchestrator activity timeout behavior.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::advance;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use support::SequenceResponder;
use support::session_fixture;
use support::status_v2_busy;
use support::status_v2_idle;

async fn test_orchestrator_server_with_long_timeout(
    mock: &MockServer,
) -> Arc<OrchestratorServerHandle> {
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
        .timeout_secs(3600)
        .build()
        .unwrap();

    Arc::new(OrchestratorServerHandle::from_server_unshared(
        opencode_orchestrator_mcp::server::OrchestratorServer::from_client_unshared(
            client,
            &base_url,
            opencode_orchestrator_mcp::server::RecoveryMode::External,
        ),
    ))
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "exercised by the ignored integration-test lane"]
async fn it_times_out_after_5_min_inactivity() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server_with_long_timeout(&mock).await;
    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));

    // Session exists
    Mock::given(method("GET"))
        .and(path("/session/s1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("s1")))
        .mount(&mock)
        .await;

    // Prompt dispatch succeeds immediately. The call counter provides a
    // deterministic synchronization point without polling the mock server.
    let prompt_responder = SequenceResponder::new(vec![ResponseTemplate::new(204)]);
    let prompt_calls = prompt_responder.call_counter();
    Mock::given(method("POST"))
        .and(path("/session/s1/prompt_async"))
        .respond_with(prompt_responder)
        .mount(&mock)
        .await;

    // The preflight status succeeds, then polling cannot observe any activity.
    // Returning idle forever would now complete through the bounded idle grace.
    let status_responder = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_idle()),
        ResponseTemplate::new(500),
    ]);
    let status_calls = status_responder.call_counter();
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_responder)
        .mount(&mock)
        .await;

    // No permissions
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

    // SSE stream stalls, forcing polling fallback
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_hours(1)),
        )
        .mount(&mock)
        .await;

    let handle = tokio::spawn(async move {
        run_tool
            .call(
                OrchestratorRunInput {
                    session_id: Some("s1".into()),
                    command: None,
                    agent: None,
                    message: Some("test prompt".into()),
                    wait_for_activity: None,
                },
                &ToolContext::default(),
            )
            .await
    });

    prompt_calls.wait_for(1).await;
    status_calls.wait_for(2).await;
    tokio::time::pause();
    advance(Duration::from_secs(301)).await;

    let result = handle.await.unwrap();
    assert!(result.is_err(), "expected inactivity timeout error");
    let err = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err.contains("idle timeout") || err.contains("no activity"),
        "expected idle-timeout wording, got: {err}"
    );
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "exercised by the ignored integration-test lane"]
async fn it_does_not_timeout_while_busy() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server_with_long_timeout(&mock).await;
    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));

    // Session exists
    Mock::given(method("GET"))
        .and(path("/session/s1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("s1")))
        .mount(&mock)
        .await;

    // Prompt dispatch succeeds immediately. The call counter provides a
    // deterministic synchronization point without polling the mock server.
    let prompt_responder = SequenceResponder::new(vec![ResponseTemplate::new(204)]);
    let prompt_calls = prompt_responder.call_counter();
    Mock::given(method("POST"))
        .and(path("/session/s1/prompt_async"))
        .respond_with(prompt_responder)
        .mount(&mock)
        .await;

    // Status always busy -> should keep resetting activity timer
    let status_responder = SequenceResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(status_v2_busy("s1")),
    ]);
    let status_calls = status_responder.call_counter();
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(status_responder)
        .mount(&mock)
        .await;

    // No permissions
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

    // SSE stream stalls, forcing polling fallback
    Mock::given(method("GET"))
        .and(path("/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_hours(1)),
        )
        .mount(&mock)
        .await;

    let mut handle = tokio::spawn(async move {
        run_tool
            .call(
                OrchestratorRunInput {
                    session_id: Some("s1".into()),
                    command: None,
                    agent: None,
                    message: Some("test prompt".into()),
                    wait_for_activity: None,
                },
                &ToolContext::default(),
            )
            .await
    });

    prompt_calls.wait_for(1).await;
    status_calls.wait_for(2).await;
    tokio::time::pause();
    advance(Duration::from_secs(301)).await;

    tokio::select! {
        result = &mut handle => {
            panic!("expected task to still be running while busy, got: {:?}", result.unwrap());
        }
        () = status_calls.wait_for(4) => {}
    }

    handle.abort();
    let _ = handle.await;
}
