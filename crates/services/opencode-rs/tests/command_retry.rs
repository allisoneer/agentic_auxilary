//! SDK-level regression test for command dispatch retry policy.
//!
//! Verifies that `messages().command()` does not retry on transport-level timeout.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use opencode_rs::ClientBuilder;
use opencode_rs::types::message::CommandRequest;
use std::time::Duration;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

/// Verify that command dispatch does not retry on transport timeout.
///
/// Setup:
/// - First request: 30s delay (causes timeout with 1s client timeout)
///
/// Expected: Timeout is returned and only one POST attempt is made.
#[tokio::test]
async fn command_does_not_retry_on_timeout() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/session/s1/command"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "executed"}))
                .set_delay(Duration::from_secs(30)),
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
    assert!(
        result.is_err(),
        "expected timeout transport error, got: {result:?}"
    );

    let received = server.received_requests().await.unwrap();
    let command_requests: Vec<_> = received
        .iter()
        .filter(|r| r.url.path() == "/session/s1/command")
        .collect();
    assert_eq!(
        command_requests.len(),
        1,
        "expected 1 command request (no timeout retry), got {}",
        command_requests.len()
    );

    let body: serde_json::Value = serde_json::from_slice(&command_requests[0].body).unwrap();
    assert!(
        body.get("messageID").is_none(),
        "expected messageID to be omitted when request.message_id is None, got {body:?}"
    );
}
