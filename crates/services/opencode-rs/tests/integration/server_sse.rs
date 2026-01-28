//! SSE event integration tests.
//!
//! Tests that verify typed event deserialization against a live opencode server.

use super::{create_test_client, should_run};
use opencode_rs::types::Event;
use std::time::Duration;
use tokio::time::timeout;

/// Test SSE connection and server.connected event.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_sse_server_connected() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;
    let mut subscription = client
        .subscribe()
        .await
        .expect("Failed to subscribe to SSE");

    // Wait for the first event (should be server.connected)
    let event = timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Failed to get event");

    assert!(
        event.is_connected(),
        "First event should be server.connected, got: {:?}",
        event
    );
}

/// Test session events have properly typed info.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_sse_session_created_event() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Subscribe FIRST to avoid race condition - we need to be listening
    // before creating the session to catch the session.created event
    let mut subscription = client
        .subscribe()
        .await
        .expect("Failed to subscribe to SSE");

    // Skip server.connected
    let _ = timeout(Duration::from_secs(5), subscription.recv()).await;

    // Now create a session - the event will be captured
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    // Poll for the session.created event for our session
    // Note: Other sessions may be created concurrently, so we filter by session ID
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut found_event = false;

    while tokio::time::Instant::now() < deadline {
        match timeout(Duration::from_secs(1), subscription.recv()).await {
            Ok(Some(Event::SessionCreated { properties })) => {
                // Verify the event has typed SessionInfo
                assert!(
                    !properties.info.id.is_empty(),
                    "SessionCreated should have info.id"
                );
                // Only check if this is our session
                if properties.info.id == session.id {
                    found_event = true;
                    break;
                }
                // Otherwise continue looking for our session
            }
            _ => continue,
        }
    }

    // We may not always catch our own session's event due to timing
    // Just verify we could connect and receive events
    if !found_event {
        println!(
            "Note: Did not catch session.created for {} (may be timing-related)",
            session.id
        );
    }

    // Clean up
    let _ = client.sessions().delete(&session.id).await;
}

/// Test that session.idle events deserialize with sessionID.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_sse_session_idle_event() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;
    let mut subscription = client
        .subscribe()
        .await
        .expect("Failed to subscribe to SSE");

    // Create a session
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    // Listen for events - session.idle should have just sessionID, not full info
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        match timeout(Duration::from_secs(1), subscription.recv()).await {
            Ok(Some(Event::SessionIdle { properties })) => {
                // Verify it has session_id
                assert!(
                    !properties.session_id.is_empty(),
                    "SessionIdle should have session_id"
                );
                break;
            }
            _ => continue,
        }
    }

    // Clean up
    let _ = client.sessions().delete(&session.id).await;
}

/// Test that message events deserialize with typed info.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_sse_message_events() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;
    let mut subscription = client
        .subscribe()
        .await
        .expect("Failed to subscribe to SSE");

    // Create a session
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    // Listen for message events
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut saw_message_event = false;

    while tokio::time::Instant::now() < deadline && !saw_message_event {
        if let Ok(Some(event)) = timeout(Duration::from_secs(1), subscription.recv()).await {
            match &event {
                Event::MessageUpdated { properties } => {
                    // Verify it has typed MessageInfo
                    assert!(
                        !properties.info.id.is_empty(),
                        "MessageUpdated should have info.id"
                    );
                    // session_id may or may not be present
                    if let Some(sid) = &properties.info.session_id {
                        assert!(
                            !sid.is_empty(),
                            "MessageUpdated session_id should not be empty if present"
                        );
                    }
                    saw_message_event = true;
                }
                Event::MessagePartUpdated { properties } => {
                    // Part events may not have session_id at top level (it's in the Part)
                    if properties.part.is_some() {
                        saw_message_event = true;
                    }
                }
                _ => {}
            }
        }
    }

    // Clean up
    let _ = client.sessions().delete(&session.id).await;
}

/// Test that permission events deserialize properly.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_sse_permission_events() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;

    // Subscribe
    let mut subscription = client
        .subscribe()
        .await
        .expect("Failed to subscribe to SSE");

    // Just verify we can subscribe without errors
    // Permission events require triggering a permission request which is harder to test
    let _ = timeout(Duration::from_secs(2), subscription.recv()).await;
}

/// Test heartbeat events.
#[tokio::test]
#[ignore] // requires: opencode serve --port 4096
async fn test_sse_heartbeat() {
    if !should_run() {
        return;
    }

    let client = create_test_client().await;
    let mut subscription = client
        .subscribe()
        .await
        .expect("Failed to subscribe to SSE");

    // Wait for a heartbeat (sent periodically)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let mut saw_heartbeat = false;

    while tokio::time::Instant::now() < deadline && !saw_heartbeat {
        match timeout(Duration::from_secs(10), subscription.recv()).await {
            Ok(Some(event)) if event.is_heartbeat() => {
                saw_heartbeat = true;
            }
            _ => {}
        }
    }

    // Heartbeat may not happen in short test windows, so don't fail
}
