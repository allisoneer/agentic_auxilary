#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use opencode_orchestrator_mcp::server::OrchestratorServer;
use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::tools::ListCommandsTool;
use opencode_orchestrator_mcp::types::ListCommandsInput;
use std::sync::Arc;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use support::SequenceResponder;
use support::commands_list_fixture;

fn test_client(base_url: &str) -> opencode_rs::Client {
    opencode_rs::ClientBuilder::new()
        .base_url(base_url)
        .timeout_secs(5)
        .build()
        .unwrap()
}

#[tokio::test]
async fn external_client_seam_still_supports_end_to_end_tool_calls() {
    let mock = MockServer::start().await;
    let server = support::test_orchestrator_server(&mock).await;

    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commands_list_fixture()))
        .mount(&mock)
        .await;

    let result = ListCommandsTool::new(Arc::clone(&server))
        .call(ListCommandsInput {}, &ToolContext::default())
        .await
        .expect("external-client seam should remain usable through the handle");

    assert_eq!(result.commands.len(), 3);
}

#[tokio::test]
async fn later_recovery_keeps_held_snapshot_stable() {
    let old_mock = MockServer::start().await;
    let new_mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "healthy": true,
                "version": "old",
            })),
            ResponseTemplate::new(500),
        ]))
        .mount(&old_mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name": "old-command", "description": "from snapshot 1"}
        ])))
        .mount(&old_mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name": "new-command", "description": "from snapshot 2"}
        ])))
        .mount(&new_mock)
        .await;

    let handle = OrchestratorServerHandle::from_server_unshared(
        OrchestratorServer::from_client_unshared(test_client(&old_mock.uri()), old_mock.uri()),
    );

    let snapshot_a = handle
        .acquire()
        .await
        .expect("first caller should receive the original snapshot");
    let snapshot_b = handle
        .acquire_or_recover_with(|| {
            let base_url = new_mock.uri();
            async move {
                Ok(OrchestratorServer::from_client_unshared(
                    test_client(&base_url),
                    base_url,
                ))
            }
        })
        .await
        .expect("second caller should recover to a rebuilt snapshot");

    let commands_a = snapshot_a.client().tools().commands().await.unwrap();
    let commands_b = snapshot_b.client().tools().commands().await.unwrap();

    assert!(!Arc::ptr_eq(&snapshot_a, &snapshot_b));
    assert_eq!(commands_a[0].name, "old-command");
    assert_eq!(commands_b[0].name, "new-command");
    assert_eq!(snapshot_a.base_url(), old_mock.uri().trim_end_matches('/'));
    assert_eq!(snapshot_b.base_url(), new_mock.uri().trim_end_matches('/'));
}
