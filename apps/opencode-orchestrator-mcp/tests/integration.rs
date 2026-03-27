//! Integration tests for `opencode-orchestrator-mcp`.
//!
//! These tests require a working `opencode` binary and are disabled by default.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use agentic_tools_core::Tool;
use opencode_orchestrator_mcp::server::OrchestratorServer;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::tools::RespondPermissionTool;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use opencode_orchestrator_mcp::types::PermissionReply;
use opencode_orchestrator_mcp::types::RespondPermissionInput;
use opencode_orchestrator_mcp::types::RunStatus;
use opencode_orchestrator_mcp::version;
use opencode_rs::types::session::CreateSessionRequest;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::OnceCell;
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

async fn start_server() -> Arc<OnceCell<OrchestratorServer>> {
    // Use the lazy init path and pre-initialize for integration tests
    let cell = Arc::new(OnceCell::new());
    let server = timeout(Duration::from_secs(30), OrchestratorServer::start_lazy())
        .await
        .expect("timeout starting embedded opencode server")
        .expect("failed to start embedded opencode server");
    cell.set(server)
        .unwrap_or_else(|_| panic!("cell should be empty"));
    cell
}

async fn create_session(server: &OnceCell<OrchestratorServer>) -> String {
    server
        .get()
        .expect("server should be initialized")
        .client()
        .sessions()
        .create(&CreateSessionRequest::default())
        .await
        .expect("failed to create session")
        .id
}

async fn cleanup_session(server: &OnceCell<OrchestratorServer>, session_id: &str) {
    if let Some(s) = server.get() {
        let _ = s.client().sessions().delete(session_id).await;
    }
}

/// Generate a unique temporary file path to avoid conflicts between test runs.
/// Uses nanosecond timestamp for uniqueness.
fn unique_tmp_path(prefix: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{nanos}.txt"))
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
        wait_for_activity: None,
    };

    let start = Instant::now();
    let result = timeout(
        Duration::from_secs(10),
        tool.call(input, &agentic_tools_core::ToolContext::default()),
    )
    .await;
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
        wait_for_activity: None,
    };

    let result = timeout(
        Duration::from_secs(180),
        tool.call(input, &agentic_tools_core::ToolContext::default()),
    )
    .await
    .expect("timed out waiting for normal completion")
    .expect("run returned error");

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
#[ignore = "requires pinned opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1 + OPENCODE_BINARY or local pinned path)"]
async fn live_managed_server_reports_exact_pinned_version() {
    if !should_run() {
        return;
    }
    init_tracing();

    let server = start_server().await;
    let s = server.get().expect("server initialized");
    let health = s.client().misc().health().await.expect("health ok");

    version::validate_exact_version(health.version.as_deref())
        .expect("version must match pinned stable");
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
        wait_for_activity: None,
    };

    let result1 = timeout(
        Duration::from_secs(180),
        tool.call(input1, &agentic_tools_core::ToolContext::default()),
    )
    .await
    .expect("timed out on first call")
    .expect("first call failed");

    assert!(matches!(result1.status, RunStatus::Completed));

    let input2 = OrchestratorRunInput {
        session_id: Some(session_id.clone()),
        command: None,
        message: Some("Say 'second' and nothing else.".into()),
        wait_for_activity: None,
    };

    let result2 = timeout(
        Duration::from_secs(180),
        tool.call(input2, &agentic_tools_core::ToolContext::default()),
    )
    .await
    .expect("timed out on second call")
    .expect("second call failed");

    cleanup_session(&server, &session_id).await;

    assert!(matches!(result2.status, RunStatus::Completed));
    assert_eq!(result2.session_id, session_id);
}

/// Test that a prompt requiring file write triggers a permission request.
///
/// This test verifies:
/// 1. Running a prompt that writes to /tmp triggers `PermissionRequired` status
/// 2. The `permission_request_id` is populated
/// 3. The response comes back within a reasonable timeout (not hanging)
#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn permission_request_returns_status() {
    if !should_run() {
        return;
    }
    init_tracing();

    let server = start_server().await;
    let session_id = create_session(&server).await;

    // Generate unique temp file path to avoid conflicts
    let tmp_file = unique_tmp_path("orch-perm-test");

    // Prompt that should trigger a file.write permission request
    let prompt = format!(
        "Create a file at '{}' with the exact content 'test'. Use the write_file tool.",
        tmp_file.display()
    );

    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));

    // Should return PermissionRequired within 60 seconds, not hang
    let result = timeout(
        Duration::from_secs(60),
        run_tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.clone()),
                command: None,
                message: Some(prompt),
                wait_for_activity: None,
            },
            &agentic_tools_core::ToolContext::default(),
        ),
    )
    .await
    .expect("timed out waiting for permission request - possible hang")
    .expect("run returned error");

    // Verify we got a permission request, not completion
    assert!(
        matches!(result.status, RunStatus::PermissionRequired),
        "expected PermissionRequired status, got {:?}",
        result.status
    );

    // Verify permission request ID is populated
    assert!(
        result.permission_request_id.is_some(),
        "permission_request_id should be set when status is PermissionRequired"
    );

    // Log for debugging
    tracing::info!(
        permission_id = ?result.permission_request_id,
        permission_type = ?result.permission_type,
        patterns = ?result.permission_patterns,
        "received permission request"
    );

    // Cleanup - best effort, don't fail test if cleanup fails
    let _ = std::fs::remove_file(&tmp_file);
    cleanup_session(&server, &session_id).await;
}

/// Test the full permission request → response → completion flow.
///
/// This test verifies:
/// 1. A prompt triggers `PermissionRequired`
/// 2. Responding with Once allows the session to continue
/// 3. The session completes (doesn't hang after permission reply)
///
/// This is the key regression test for Bug 1 (race conditions causing hangs).
/// Pre-fix, this test will timeout. Post-fix, it should complete reliably.
#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn permission_response_resumes_and_completes() {
    const MAX_PERMISSION_ROUNDS: usize = 5;

    if !should_run() {
        return;
    }
    init_tracing();

    let server = start_server().await;
    let session_id = create_session(&server).await;

    let tmp_file = unique_tmp_path("orch-perm-flow");
    let prompt = format!(
        "Create a file at '{}' containing exactly 'hello'. Use write_file tool.",
        tmp_file.display()
    );

    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));
    let respond_tool = RespondPermissionTool::new(Arc::clone(&server));

    // Step 1: Trigger permission request
    let result1 = timeout(
        Duration::from_secs(60),
        run_tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.clone()),
                command: None,
                message: Some(prompt),
                wait_for_activity: None,
            },
            &agentic_tools_core::ToolContext::default(),
        ),
    )
    .await
    .expect("timed out waiting for initial permission request")
    .expect("run failed");

    assert!(
        matches!(result1.status, RunStatus::PermissionRequired),
        "expected PermissionRequired, got {:?}",
        result1.status
    );

    tracing::info!(
        permission_id = ?result1.permission_request_id,
        "received permission request, responding with Once"
    );

    // Step 2: Respond to permission and wait for completion
    // This is where Bug 1 causes hangs - the race between permission reply
    // and SSE subscription can cause SessionIdle to be missed.
    //
    // We use a loop to handle potential multiple permissions (e.g., directory + file)
    let current_session_id = session_id.clone();
    let mut attempts = 0;

    loop {
        attempts += 1;
        assert!(
            attempts <= MAX_PERMISSION_ROUNDS,
            "exceeded {MAX_PERMISSION_ROUNDS} permission rounds - possible infinite permission loop"
        );

        let respond_result = timeout(
            Duration::from_secs(120),
            respond_tool.call(
                RespondPermissionInput {
                    session_id: current_session_id.clone(),
                    permission_request_id: None,
                    reply: PermissionReply::Once,
                    message: Some(format!("test approval round {attempts}")),
                },
                &agentic_tools_core::ToolContext::default(),
            ),
        )
        .await
        .expect("REGRESSION: timed out after permission reply - Bug 1 hang detected")
        .expect("respond_permission failed");

        match respond_result.status {
            RunStatus::Completed => {
                tracing::info!(
                    response = ?respond_result.response,
                    "session completed successfully after {attempts} permission round(s)"
                );
                // Assert response is present and non-empty after permission approval
                let resp = respond_result
                    .response
                    .as_deref()
                    .expect("expected response after permission approval");
                assert!(
                    !resp.trim().is_empty(),
                    "response should not be empty after permission approval"
                );
                break;
            }
            RunStatus::PermissionRequired => {
                tracing::info!(
                    permission_id = ?respond_result.permission_request_id,
                    permission_type = ?respond_result.permission_type,
                    "additional permission required, continuing loop"
                );
                // Continue to next iteration
            }
            RunStatus::QuestionRequired => {
                panic!(
                    "unexpected question interruption in permission flow: {:?}",
                    respond_result.questions
                );
            }
        }
    }

    // Verify the file was created (optional - confirms the work was done)
    if tmp_file.exists() {
        let contents = std::fs::read_to_string(&tmp_file).unwrap_or_default();
        tracing::info!(file = %tmp_file.display(), contents = %contents, "file created");
    }

    // Cleanup
    let _ = std::fs::remove_file(&tmp_file);
    cleanup_session(&server, &session_id).await;
}

/// Test that rejecting a permission returns response=None with appropriate warning.
///
/// This validates that rejection doesn't return stale pre-rejection text.
#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn permission_reject_returns_none_with_warning() {
    if !should_run() {
        return;
    }
    init_tracing();

    let server = start_server().await;
    let session_id = create_session(&server).await;

    let tmp_file = unique_tmp_path("orch-reject-test");
    let prompt = format!(
        "Create a file at '{}' containing 'test'. Use write_file tool.",
        tmp_file.display()
    );

    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));
    let respond_tool = RespondPermissionTool::new(Arc::clone(&server));

    // Step 1: Trigger permission request
    let result = timeout(
        Duration::from_secs(60),
        run_tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.clone()),
                command: None,
                message: Some(prompt),
                wait_for_activity: None,
            },
            &agentic_tools_core::ToolContext::default(),
        ),
    )
    .await
    .expect("timed out waiting for permission request")
    .expect("run failed");

    assert!(
        matches!(result.status, RunStatus::PermissionRequired),
        "expected PermissionRequired, got {:?}",
        result.status
    );

    // Step 2: Reject the permission
    let reject_result = timeout(
        Duration::from_secs(60),
        respond_tool.call(
            RespondPermissionInput {
                session_id: session_id.clone(),
                permission_request_id: None,
                reply: PermissionReply::Reject,
                message: None,
            },
            &agentic_tools_core::ToolContext::default(),
        ),
    )
    .await
    .expect("timed out after rejection")
    .expect("respond_permission failed");

    // Assert rejection behavior
    assert!(
        matches!(reject_result.status, RunStatus::Completed),
        "expected Completed after rejection, got {:?}",
        reject_result.status
    );
    assert!(
        reject_result.response.is_none(),
        "expected response=None after rejection, got {:?}",
        reject_result.response
    );
    assert!(
        reject_result
            .warnings
            .iter()
            .any(|w| w.to_lowercase().contains("reject")),
        "expected warning about rejection, got {:?}",
        reject_result.warnings
    );

    tracing::info!(
        warnings = ?reject_result.warnings,
        "rejection completed with expected warnings"
    );

    // Cleanup
    let _ = std::fs::remove_file(&tmp_file);
    cleanup_session(&server, &session_id).await;
}
