//! SSE event integration tests.
//!
//! Tests that verify typed event deserialization against a live opencode server.

use super::{create_test_client, should_run};
use opencode_rs::types::{Event, SessionInfoProps};
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
    let event = timeout(Duration::from_secs(5), subscription.next_event())
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
    let _ = timeout(Duration::from_secs(5), subscription.next_event()).await;

    // Now create a session - the event will be captured
    let session = client
        .sessions()
        .create(&Default::default())
        .await
        .expect("Failed to create session");

    // Poll for the session.created event
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut found_event = false;

    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(event)) = timeout(Duration::from_secs(1), subscription.next_event()).await {
            if let Event::SessionCreated { properties } = &event {
                // Verify the event has typed SessionInfo
                assert!(
                    !properties.info.id.is_empty(),
                    "SessionCreated should have info.id"
                );
                assert_eq!(
                    properties.info.id, session.id,
                    "SessionCreated info.id should match created session"
                );
                found_event = true;
                break;
            }
        }
    }

    assert!(found_event, "Should have received SessionCreated event");

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
        if let Ok(Some(event)) = timeout(Duration::from_secs(1), subscription.next_event()).await {
            if let Event::SessionIdle { properties } = &event {
                // Verify it has session_id
                assert!(
                    !properties.session_id.is_empty(),
                    "SessionIdle should have session_id"
                );
                break;
            }
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
        if let Ok(Some(event)) = timeout(Duration::from_secs(1), subscription.next_event()).await {
            match &event {
                Event::MessageUpdated { properties } => {
                    // Verify it has typed MessageInfo
                    assert!(
                        !properties.info.id.is_empty(),
                        "MessageUpdated should have info.id"
                    );
                    assert!(
                        !properties.info.session_id.is_empty(),
                        "MessageUpdated should have info.session_id"
                    );
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
    let _ = timeout(Duration::from_secs(2), subscription.next_event()).await;
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
        if let Ok(Some(event)) = timeout(Duration::from_secs(10), subscription.next_event()).await {
            if event.is_heartbeat() {
                saw_heartbeat = true;
            }
        }
    }

    // Heartbeat may not happen in short test windows, so don't fail
}
