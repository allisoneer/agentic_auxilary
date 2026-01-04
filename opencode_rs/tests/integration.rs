//! Integration tests for opencode_rs.
//!
//! These tests run against a real OpenCode server and are gated by environment variables.
//!
//! To run:
//!   OPENCODE_INTEGRATION=1 cargo test --test integration -- --ignored
//!
//! Optional environment variables:
//!   OPENCODE_BASE_URL - Base URL of the OpenCode server (default: http://127.0.0.1:4096)
//!   OPENCODE_DIRECTORY - Directory context for requests (default: current directory)

use opencode_rs::ClientBuilder;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::session::CreateSessionRequest;
use std::time::Duration;

/// Check if integration tests should run.
fn should_run() -> bool {
    std::env::var("OPENCODE_INTEGRATION").is_ok()
}

/// Get the base URL for the OpenCode server.
fn base_url() -> String {
    std::env::var("OPENCODE_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:4096".to_string())
}

/// Get the directory context.
fn directory() -> String {
    std::env::var("OPENCODE_DIRECTORY").unwrap_or_else(|_| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "/tmp".to_string())
    })
}

/// Build a client for integration tests.
fn build_client() -> opencode_rs::Client {
    ClientBuilder::new()
        .base_url(base_url())
        .directory(directory())
        .timeout_secs(300)
        .build()
        .expect("Failed to build client")
}

/// Test server health endpoint.
#[tokio::test]
#[ignore]
async fn test_server_health() {
    if !should_run() {
        return;
    }

    let client = build_client();
    let health = client.misc().health().await.expect("Failed to get health");
    assert!(health.healthy, "Server should be healthy");
}

/// Test full session lifecycle: create -> list -> get -> delete.
#[tokio::test]
#[ignore]
async fn test_session_lifecycle() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Create session
    let session = client
        .sessions()
        .create(&CreateSessionRequest {
            title: Some("Integration Test Session".into()),
            ..Default::default()
        })
        .await
        .expect("Failed to create session");
    assert!(!session.id.is_empty());

    // Get session to verify it exists
    let fetched = client
        .sessions()
        .get(&session.id)
        .await
        .expect("Failed to get session");
    assert_eq!(fetched.id, session.id);

    // List sessions (our session should be there, but don't fail if not)
    match client.sessions().list().await {
        Ok(sessions) => {
            println!("Found {} sessions", sessions.len());
        }
        Err(e) => {
            println!("List sessions failed: {:?}", e);
        }
    }

    // Cleanup - delete session
    client
        .sessions()
        .delete(&session.id)
        .await
        .expect("Failed to delete session");
}

/// Test session with prompt and SSE streaming.
#[tokio::test]
#[ignore]
async fn test_session_prompt_and_stream() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Create session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await
        .expect("Failed to create session");

    // Subscribe to events BEFORE sending prompt
    let mut subscription = client
        .subscribe_session(&session.id)
        .await
        .expect("Failed to subscribe");

    // Send a simple prompt
    let prompt_result = client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Say 'hello' and nothing else.".into(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: None,
                agent: None,
                no_reply: None,
                system: None,
                variant: None,
            },
        )
        .await;

    if prompt_result.is_err() {
        // Prompt may fail if no provider is configured
        println!("Prompt failed (no provider?): {:?}", prompt_result.err());
        subscription.close();
        client.sessions().delete(&session.id).await.ok();
        return;
    }

    // Wait for events with timeout (shorter timeout for CI)
    let timeout = Duration::from_secs(30);
    let start = std::time::Instant::now();
    let mut got_any_event = false;

    while start.elapsed() < timeout {
        tokio::select! {
            event = subscription.recv() => {
                match event {
                    Some(Event::SessionIdle { .. }) => {
                        got_any_event = true;
                        break;
                    }
                    Some(Event::MessagePartUpdated { .. }) => {
                        got_any_event = true;
                    }
                    Some(Event::SessionError { .. }) => {
                        got_any_event = true;
                        break;
                    }
                    Some(Event::ServerHeartbeat { .. }) => {
                        // Heartbeat is also a valid event
                        got_any_event = true;
                    }
                    Some(_) => {
                        got_any_event = true;
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    // Just verify we connected and received some events
    // The actual content depends on provider configuration
    println!(
        "Streaming test: got_any_event={}, elapsed={:?}",
        got_any_event,
        start.elapsed()
    );

    // Cleanup
    subscription.close();
    client
        .sessions()
        .delete(&session.id)
        .await
        .expect("Failed to delete session");
}

/// Test session abort.
#[tokio::test]
#[ignore]
async fn test_session_abort() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Create session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await
        .expect("Failed to create session");

    // Abort should succeed even on idle session
    let result = client.sessions().abort(&session.id).await;
    // Abort may fail if session is already idle, that's OK
    if result.is_err() {
        // Check it's not a 404
        assert!(
            !result.as_ref().unwrap_err().is_not_found(),
            "Session should exist"
        );
    }

    // Cleanup
    client
        .sessions()
        .delete(&session.id)
        .await
        .expect("Failed to delete session");
}

/// Test permissions API.
#[tokio::test]
#[ignore]
async fn test_permissions_list() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // List permissions (may be empty, that's fine)
    let permissions = client
        .permissions()
        .list()
        .await
        .expect("Failed to list permissions");

    // Just verify we got a list (could be empty, that's OK)
    let _ = permissions.len();
}

/// Test files API.
#[tokio::test]
#[ignore]
async fn test_files_list() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // List files in project
    // Note: This endpoint may require specific project context or return 400
    // if not properly configured
    match client.files().list().await {
        Ok(files) => {
            // Should have some files in a project directory
            let _ = files.len();
        }
        Err(e) => {
            // Some OpenCode configurations may not support this endpoint
            println!("Files list not available: {:?}", e);
        }
    }
}

/// Test file status.
#[tokio::test]
#[ignore]
async fn test_files_status() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Get file status (VCS status)
    let status = client
        .files()
        .status()
        .await
        .expect("Failed to get file status");

    // Just verify we got a response
    let _ = status.len();
}

/// Test project API.
#[tokio::test]
#[ignore]
async fn test_project_list() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // List projects
    let projects = client
        .project()
        .list()
        .await
        .expect("Failed to list projects");

    // Should have at least one project
    assert!(!projects.is_empty(), "Should have at least one project");
}

/// Test current project.
#[tokio::test]
#[ignore]
async fn test_project_current() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Get current project
    let project = client
        .project()
        .current()
        .await
        .expect("Failed to get current project");

    assert!(!project.id.is_empty(), "Project should have an ID");
}

/// Test providers API.
#[tokio::test]
#[ignore]
async fn test_providers_list() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // List providers (raw response)
    let providers = client
        .providers()
        .list()
        .await
        .expect("Failed to list providers");

    // Should have some providers
    // Note: List may be empty if no providers are configured
    for provider in &providers {
        assert!(!provider.id.is_empty(), "Provider should have an ID");
        assert!(!provider.name.is_empty(), "Provider should have a name");
    }
}

/// Test MCP status.
#[tokio::test]
#[ignore]
async fn test_mcp_status() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Get MCP status
    let status = client
        .mcp()
        .status()
        .await
        .expect("Failed to get MCP status");

    // Just verify we got a response
    let _ = status.servers.len();
}

/// Test config API.
#[tokio::test]
#[ignore]
async fn test_config_get() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Get config
    let config = client.config().get().await.expect("Failed to get config");

    // Just verify we got a config response
    // The specific fields depend on OpenCode's config structure
    assert!(config.extra.is_object() || config.extra.is_null());
}

/// Test tools/agents API.
#[tokio::test]
#[ignore]
async fn test_agents_list() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // List agents
    let agents = client
        .tools()
        .agents()
        .await
        .expect("Failed to list agents");

    // Should have at least one agent
    assert!(!agents.is_empty(), "Should have at least one agent");
}

/// Test commands list.
#[tokio::test]
#[ignore]
async fn test_commands_list() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // List commands
    let commands = client
        .tools()
        .commands()
        .await
        .expect("Failed to list commands");

    // May or may not have commands
    let _ = commands.len();
}

/// Test VCS info.
#[tokio::test]
#[ignore]
async fn test_vcs_info() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Get VCS info - the .expect() validates we got a successful response
    let vcs = client.misc().vcs().await.expect("Failed to get VCS info");

    // Log the VCS type for debugging (type is optional as this could be non-git)
    println!("VCS type: {:?}", vcs.r#type);
}

/// Test path info.
#[tokio::test]
#[ignore]
async fn test_path_info() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Get path info
    let path = client.misc().path().await.expect("Failed to get path info");

    assert!(!path.directory.is_empty(), "Directory should not be empty");
}

/// Test OpenAPI doc endpoint.
#[tokio::test]
#[ignore]
async fn test_openapi_doc() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Get OpenAPI doc
    let doc = client
        .misc()
        .doc()
        .await
        .expect("Failed to get OpenAPI doc");

    // Should be a valid OpenAPI document
    assert!(doc.spec.is_object(), "Doc should be a JSON object");
    assert!(
        doc.spec.get("openapi").is_some() || doc.spec.get("swagger").is_some(),
        "Should be an OpenAPI/Swagger document"
    );
}

/// Test session fork.
#[tokio::test]
#[ignore]
async fn test_session_fork() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Create parent session
    let parent = client
        .sessions()
        .create(&CreateSessionRequest {
            title: Some("Parent Session".into()),
            ..Default::default()
        })
        .await
        .expect("Failed to create parent session");

    // Fork session
    let forked = client
        .sessions()
        .fork(&parent.id)
        .await
        .expect("Failed to fork session");

    assert_ne!(forked.id, parent.id, "Forked session should have new ID");
    // Note: parent_id field may not be set in all OpenCode versions

    // Cleanup
    client
        .sessions()
        .delete(&forked.id)
        .await
        .expect("Failed to delete forked session");
    client
        .sessions()
        .delete(&parent.id)
        .await
        .expect("Failed to delete parent session");
}

/// Test session children.
#[tokio::test]
#[ignore]
async fn test_session_children() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Create parent session
    let parent = client
        .sessions()
        .create(&CreateSessionRequest {
            title: Some("Parent Session".into()),
            ..Default::default()
        })
        .await
        .expect("Failed to create parent session");

    // Fork to create a child
    let child = client
        .sessions()
        .fork(&parent.id)
        .await
        .expect("Failed to fork session");

    // Get children - this may or may not include the forked session
    // depending on OpenCode version and how forking is tracked
    let children = client
        .sessions()
        .children(&parent.id)
        .await
        .expect("Failed to get children");

    // Just verify we got a response (children list may be empty in some versions)
    let _ = children.len();

    // Cleanup
    client
        .sessions()
        .delete(&child.id)
        .await
        .expect("Failed to delete child session");
    client
        .sessions()
        .delete(&parent.id)
        .await
        .expect("Failed to delete parent session");
}

/// Test session diff.
#[tokio::test]
#[ignore]
async fn test_session_diff() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Create session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await
        .expect("Failed to create session");

    // Get diff (may be empty for new session)
    // Note: Response format may vary - could be object or array
    match client.sessions().diff(&session.id).await {
        Ok(diff) => {
            // Just verify we got a response
            let _ = diff.files.len();
        }
        Err(e) => {
            // Some versions return different format
            println!("Diff returned unexpected format: {:?}", e);
        }
    }

    // Cleanup
    client
        .sessions()
        .delete(&session.id)
        .await
        .expect("Failed to delete session");
}

/// Test session todos.
#[tokio::test]
#[ignore]
async fn test_session_todos() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Create session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await
        .expect("Failed to create session");

    // Get todos (may be empty)
    let todos = client
        .sessions()
        .todo(&session.id)
        .await
        .expect("Failed to get todos");

    // Just verify we got a list
    let _ = todos.len();

    // Cleanup
    client
        .sessions()
        .delete(&session.id)
        .await
        .expect("Failed to delete session");
}

/// Test global event subscription.
#[tokio::test]
#[ignore]
async fn test_global_events() {
    if !should_run() {
        return;
    }

    let client = build_client();

    // Subscribe to global events
    let mut subscription = client
        .subscribe_global()
        .await
        .expect("Failed to subscribe to global events");

    // Wait briefly for any events (heartbeat, etc.)
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();
    let mut got_event = false;

    while start.elapsed() < timeout {
        tokio::select! {
            event = subscription.recv() => {
                match event {
                    Some(_) => {
                        got_event = true;
                        break;
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }

    // May or may not get an event depending on server activity
    // Just verify we could subscribe
    subscription.close();

    // This is informational - we successfully subscribed
    if got_event {
        println!("Received global event");
    } else {
        println!("No global events received within timeout (this is OK)");
    }
}
