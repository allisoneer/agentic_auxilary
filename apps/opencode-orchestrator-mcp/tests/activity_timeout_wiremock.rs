//! Wiremock-based integration tests for orchestrator activity timeout behavior.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use agentic_tools_core::{Tool, ToolContext};
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::time::advance;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::{session_fixture, status_v2_busy, status_v2_idle};

fn test_orchestrator_server_with_long_timeout(
    mock: &MockServer,
) -> Arc<OnceCell<opencode_orchestrator_mcp::server::OrchestratorServer>> {
    let base_url = mock.uri().trim_end_matches('/').to_string();
    let client = opencode_rs::ClientBuilder::new()
        .base_url(&base_url)
        .timeout_secs(3600)
        .build()
        .unwrap();

    let cell = Arc::new(OnceCell::new());
    cell.set(
        opencode_orchestrator_mcp::server::OrchestratorServer::from_client_unshared(
            client, &base_url,
        ),
    )
    .unwrap_or_else(|_| panic!("cell should be empty"));
    cell
}

async fn wait_for_request_path(mock: &MockServer, expected_path: &str) {
    for _ in 0..200 {
        if let Some(requests) = mock.received_requests().await
            && requests.iter().any(|req| req.url.path() == expected_path)
        {
            return;
        }
        tokio::task::yield_now().await;
    }

    panic!("timed out waiting for request path: {expected_path}");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[ignore = "tokio paused time + reqwest/wiremock interactions are flaky in this harness"]
async fn it_times_out_after_5_min_inactivity() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server_with_long_timeout(&mock);
    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));

    // Session exists
    Mock::given(method("GET"))
        .and(path("/session/s1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("s1")))
        .mount(&mock)
        .await;

    // Prompt dispatch succeeds immediately
    Mock::given(method("POST"))
        .and(path("/session/s1/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    // Status always idle -> no activity
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
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
                .set_delay(Duration::from_secs(3600)),
        )
        .mount(&mock)
        .await;

    let handle = tokio::spawn(async move {
        run_tool
            .call(
                OrchestratorRunInput {
                    session_id: Some("s1".into()),
                    command: None,
                    message: Some("test prompt".into()),
                    wait_for_activity: None,
                },
                &ToolContext::default(),
            )
            .await
    });

    wait_for_request_path(&mock, "/session/s1").await;
    advance(Duration::from_secs(301)).await;
    tokio::task::yield_now().await;

    let result = handle.await.unwrap();
    assert!(result.is_err(), "expected inactivity timeout error");
    let err = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err.contains("idle timeout") || err.contains("no activity"),
        "expected idle-timeout wording, got: {err}"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[ignore = "tokio paused time + reqwest/wiremock interactions are flaky in this harness"]
async fn it_does_not_timeout_while_busy() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server_with_long_timeout(&mock);
    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));

    // Session exists
    Mock::given(method("GET"))
        .and(path("/session/s1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("s1")))
        .mount(&mock)
        .await;

    // Prompt dispatch succeeds immediately
    Mock::given(method("POST"))
        .and(path("/session/s1/prompt_async"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    // Status always busy -> should keep resetting activity timer
    Mock::given(method("GET"))
        .and(path("/session/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_busy("s1")))
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
                .set_delay(Duration::from_secs(3600)),
        )
        .mount(&mock)
        .await;

    let handle = tokio::spawn(async move {
        run_tool
            .call(
                OrchestratorRunInput {
                    session_id: Some("s1".into()),
                    command: None,
                    message: Some("test prompt".into()),
                    wait_for_activity: None,
                },
                &ToolContext::default(),
            )
            .await
    });

    wait_for_request_path(&mock, "/session/s1").await;
    advance(Duration::from_secs(301)).await;
    tokio::task::yield_now().await;

    if handle.is_finished() {
        let result = handle.await.unwrap();
        panic!("expected task to still be running while busy, got: {result:?}");
    }

    handle.abort();
    let _ = handle.await;
}
