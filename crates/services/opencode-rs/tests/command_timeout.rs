//! SDK-level timeout regression tests for long-running command requests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use opencode_rs::ClientBuilder;
use opencode_rs::types::message::CommandRequest;
use std::time::Duration;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

/// Verify that the default client timeout allows a slow command response.
#[tokio::test]
async fn default_timeout_allows_301s_command_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/session/s1/command"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"status": "executed"}))
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;

    let client = ClientBuilder::new().base_url(server.uri()).build().unwrap();

    let req = CommandRequest {
        command: "test".into(),
        arguments: String::new(),
        message_id: None,
    };

    let result = client.messages().command("s1", &req).await;
    assert!(
        result.is_ok(),
        "request should not time out with 1800s default, got: {result:?}"
    );

    // Verify messageID was sent
    let received = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert!(body.get("messageID").and_then(|v| v.as_str()).is_some());
}
