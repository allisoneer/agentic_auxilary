//! Integration tests for `opencode-orchestrator-mcp`.
//!
//! These tests require a working `opencode` binary and are disabled by default.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use opencode_orchestrator_mcp::server::OrchestratorServer;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::types::{OrchestratorRunInput, RunStatus};
use opencode_rs::types::session::CreateSessionRequest;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

fn should_run() -> bool {
    std::env::var("OPENCODE_ORCHESTRATOR_INTEGRATION").is_ok()
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("opencode_orchestrator_mcp=debug".parse().unwrap()),
        )
        .with_test_writer()
        .try_init();
}

async fn start_server() -> Arc<OrchestratorServer> {
    timeout(Duration::from_secs(30), OrchestratorServer::start())
        .await
        .expect("timeout starting embedded opencode server")
        .expect("failed to start embedded opencode server")
}

async fn create_session(server: &OrchestratorServer) -> String {
    server
        .client()
        .sessions()
        .create(&CreateSessionRequest::default())
        .await
        .expect("failed to create session")
        .id
}

async fn cleanup_session(server: &OrchestratorServer, session_id: &str) {
    let _ = server.client().sessions().delete(session_id).await;
}

#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn unknown_command_errors_fast() {
    if !should_run() {
        return;
    }
    init_tracing();

    let server = start_server().await;
    let session_id = create_session(&server).await;

    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let input = OrchestratorRunInput {
        session_id: Some(session_id.clone()),
        command: Some("___definitely_not_a_real_command___".into()),
        message: Some("test argument".into()),
    };

    let start = Instant::now();
    let result = timeout(Duration::from_secs(10), tool.run_impl(input)).await;
    let elapsed = start.elapsed();

    cleanup_session(&server, &session_id).await;

    let result = result.expect("REGRESSION: timed out waiting for error (should fail fast)");
    assert!(
        result.is_err(),
        "expected error for unknown command, got: {result:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "should return error quickly, took {elapsed:?}",
    );
}

#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn prompt_completes_and_extracts_response() {
    if !should_run() {
        return;
    }
    init_tracing();

    let server = start_server().await;
    let session_id = create_session(&server).await;

    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let input = OrchestratorRunInput {
        session_id: Some(session_id.clone()),
        command: None,
        message: Some("Say exactly 'hello' and nothing else.".into()),
    };

    let result = timeout(Duration::from_secs(180), tool.run_impl(input))
        .await
        .expect("timed out waiting for normal completion")
        .expect("orchestrator_run returned error");

    cleanup_session(&server, &session_id).await;

    assert_eq!(result.session_id, session_id, "session_id should match");
    assert!(
        matches!(result.status, RunStatus::Completed),
        "expected Completed status, got {:?}",
        result.status
    );

    let response = result
        .response
        .expect("expected a response for completed session");
    assert!(!response.trim().is_empty(), "response should not be empty");
}

#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn session_resumption_works() {
    if !should_run() {
        return;
    }
    init_tracing();

    let server = start_server().await;
    let session_id = create_session(&server).await;

    let tool = OrchestratorRunTool::new(Arc::clone(&server));

    let input1 = OrchestratorRunInput {
        session_id: Some(session_id.clone()),
        command: None,
        message: Some("Say 'first' and nothing else.".into()),
    };

    let result1 = timeout(Duration::from_secs(180), tool.run_impl(input1))
        .await
        .expect("timed out on first call")
        .expect("first call failed");

    assert!(matches!(result1.status, RunStatus::Completed));

    let input2 = OrchestratorRunInput {
        session_id: Some(session_id.clone()),
        command: None,
        message: Some("Say 'second' and nothing else.".into()),
    };

    let result2 = timeout(Duration::from_secs(180), tool.run_impl(input2))
        .await
        .expect("timed out on second call")
        .expect("second call failed");

    cleanup_session(&server, &session_id).await;

    assert!(matches!(result2.status, RunStatus::Completed));
    assert_eq!(result2.session_id, session_id);
}
