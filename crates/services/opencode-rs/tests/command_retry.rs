//! SDK-level regression test for command dispatch retry.
//!
//! Verifies that `messages().command()` retries on transport-level timeout.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use opencode_rs::ClientBuilder;
use opencode_rs::types::message::CommandRequest;
use std::time::Duration;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

/// Verify that command dispatch retries on transport timeout.
///
/// Setup:
/// - First request: 30s delay (causes timeout with 1s client timeout)
/// - Second request: immediate success
///
/// Expected: Second request succeeds after retry.
#[tokio::test]
async fn command_retries_on_timeout() {
    let server = MockServer::start().await;

    // First request: timeout (30s delay > 1s client timeout)
    Mock::given(method("POST"))
        .and(path("/session/s1/command"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "executed"}))
                .set_delay(Duration::from_secs(30)),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // Second request: immediate success
    Mock::given(method("POST"))
        .and(path("/session/s1/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "executed"})),
        )
        .mount(&server)
        .await;

    let client = ClientBuilder::new()
        .base_url(server.uri())
        .timeout_secs(1)
        .build()
        .unwrap();

    let req = CommandRequest {
        command: "test".into(),
        arguments: String::new(),
        message_id: None,
    };

    let result = client.messages().command("s1", &req).await;
    assert!(result.is_ok(), "expected retry to succeed, got: {result:?}");

    // Verify 2 requests made (1 timeout + 1 success)
    let received = server.received_requests().await.unwrap();
    let command_requests: Vec<_> = received
        .iter()
        .filter(|r| r.url.path() == "/session/s1/command")
        .collect();
    assert_eq!(
        command_requests.len(),
        2,
        "expected 2 command requests (1 timeout + 1 success), got {}",
        command_requests.len()
    );

    // Verify both retry attempts share identical messageId
    let b1: serde_json::Value = serde_json::from_slice(&command_requests[0].body).unwrap();
    let b2: serde_json::Value = serde_json::from_slice(&command_requests[1].body).unwrap();

    let mid1 = b1.get("messageId").and_then(|v| v.as_str()).unwrap();
    let mid2 = b2.get("messageId").and_then(|v| v.as_str()).unwrap();
    assert!(!mid1.is_empty());
    assert_eq!(mid1, mid2, "expected stable messageId across retries");
}
