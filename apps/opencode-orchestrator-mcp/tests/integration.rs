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
use std::path::Path;
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

const PERMISSION_CONFIG_FIXTURE: &str = "opencode.permission.config.json";

// NOTE: This fixture pins a concrete model ID (currently anthropic/claude-sonnet-4-5).
// OpenCode v1.14.19 resolves model availability dynamically at runtime. If this pin is invalid
// or unavailable, the server should fail loudly (no silent fallback). If needed, update the
// fixture model string to another concrete (non-*-latest) model ID.
struct TempFileGuard {
    path: std::path::PathBuf,
}

impl TempFileGuard {
    fn new(prefix: &str, contents: &str) -> Self {
        let path = unique_tmp_path(prefix);
        std::fs::write(&path, contents)
            .unwrap_or_else(|e| panic!("failed to write temp file {}: {e}", path.display()));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn load_fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

fn external_read_prompt(file_path: &Path) -> String {
    format!(
        "There is a text file at \"{path}\".\n\
         The file contains a unique token.\n\
         Use the `read` tool to read it, then reply with the exact file contents and nothing else.\n\
         Do not guess the contents.",
        path = file_path.display(),
    )
}

#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn unknown_command_errors_fast() {
    if !should_run() {
        return;
    }
    init_tracing();

    let config_json = load_fixture("opencode.permission.config.json");
    let server = start_server_with_config(config_json).await;
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

/// Test that reading an external file triggers a permission request.
///
/// This test verifies:
/// 1. Running a prompt that reads from /tmp triggers `PermissionRequired` status
/// 2. The `permission_request_id` is populated
/// 3. The response comes back within a reasonable timeout (not hanging)
#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn permission_request_returns_status() {
    if !should_run() {
        return;
    }
    init_tracing();

    let config_json = load_fixture(PERMISSION_CONFIG_FIXTURE);
    let server = start_server_with_config(config_json).await;
    let session_id = create_session(&server).await;

    let token = format!(
        "opencode-orchestrator-mcp permission token: {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos()
    );
    let tmp_file = TempFileGuard::new("orch-perm-test", &token);
    let prompt = external_read_prompt(tmp_file.path());

    let run_tool = OrchestratorRunTool::new(Arc::clone(&server));

    // Should return PermissionRequired within 120 seconds, not hang
    // (Model inference time is variable; 60s was too tight)
    let result = timeout(
        Duration::from_secs(120),
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
    assert_eq!(
        result.permission_type.as_deref(),
        Some("external_directory")
    );

    // Log for debugging
    tracing::info!(
        permission_id = ?result.permission_request_id,
        permission_type = ?result.permission_type,
        patterns = ?result.permission_patterns,
        "received permission request"
    );

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

    let config_json = load_fixture(PERMISSION_CONFIG_FIXTURE);
    let server = start_server_with_config(config_json).await;
    let session_id = create_session(&server).await;

    let token = format!(
        "opencode-orchestrator-mcp permission flow token: {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos()
    );
    let tmp_file = TempFileGuard::new("orch-perm-flow", &token);
    let prompt = external_read_prompt(tmp_file.path());

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
    assert_eq!(
        result1.permission_type.as_deref(),
        Some("external_directory")
    );

    tracing::info!(
        permission_id = ?result1.permission_request_id,
        "received permission request, responding with Once"
    );

    // Step 2: Respond to permission and wait for completion
    // This is where Bug 1 causes hangs - the race between permission reply
    // and SSE subscription can cause SessionIdle to be missed.
    //
    // We use a loop to handle potential multiple permissions if the runtime config changes.
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
                    resp.contains(&token),
                    "expected response to include token, got {resp:?}"
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

    let config_json = load_fixture(PERMISSION_CONFIG_FIXTURE);
    let server = start_server_with_config(config_json).await;
    let session_id = create_session(&server).await;

    let token = format!(
        "opencode-orchestrator-mcp permission reject token: {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos()
    );
    let tmp_file = TempFileGuard::new("orch-reject-test", &token);
    let prompt = external_read_prompt(tmp_file.path());

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
    assert_eq!(
        result.permission_type.as_deref(),
        Some("external_directory")
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

    cleanup_session(&server, &session_id).await;
}

/// Start a server with custom config injection.
async fn start_server_with_config(config_json: String) -> Arc<OnceCell<OrchestratorServer>> {
    let cell = Arc::new(OnceCell::new());
    let server = timeout(
        Duration::from_secs(30),
        OrchestratorServer::start_lazy_with_config(Some(config_json)),
    )
    .await
    .expect("timeout starting embedded opencode server with config")
    .expect("failed to start embedded opencode server with config");
    cell.set(server)
        .unwrap_or_else(|_| panic!("cell should be empty"));
    cell
}

/// Live integration test for question tool flow.
///
/// This test validates that the config injection infrastructure works.
/// Triggering an actual question requires specific prompts; this test
/// validates the server starts correctly with injected config.
#[tokio::test]
#[ignore = "requires opencode binary (set OPENCODE_ORCHESTRATOR_INTEGRATION=1)"]
async fn live_question_tool_infrastructure() {
    if !should_run() {
        return;
    }
    init_tracing();

    // Load test config fixture
    let config_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opencode.config.json");
    let config_json = load_fixture("opencode.config.json");

    tracing::info!("Loaded config from {}", config_path.display());

    // Start server with injected config
    let server = start_server_with_config(config_json).await;

    // Create session to verify server is working
    let session_id = create_session(&server).await;

    tracing::info!(session_id = %session_id, "Created session with config-injected server");

    // Run a simple prompt to verify the server is functional
    let tool = OrchestratorRunTool::new(Arc::clone(&server));
    let result = timeout(
        Duration::from_secs(60),
        tool.call(
            OrchestratorRunInput {
                session_id: Some(session_id.clone()),
                command: None,
                message: Some("Say 'config test passed' and nothing else.".into()),
                wait_for_activity: None,
            },
            &agentic_tools_core::ToolContext::default(),
        ),
    )
    .await
    .expect("timed out waiting for response")
    .expect("run failed");

    assert!(
        matches!(result.status, RunStatus::Completed),
        "expected Completed status, got {:?}",
        result.status
    );

    tracing::info!(
        response = ?result.response,
        "Config injection test completed successfully"
    );

    // Cleanup
    cleanup_session(&server, &session_id).await;
}
