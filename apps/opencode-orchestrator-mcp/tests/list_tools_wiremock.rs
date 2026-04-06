//! Wiremock tests for `list_sessions` and `list_commands` tools.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use opencode_orchestrator_mcp::tools::ListCommandsTool;
use opencode_orchestrator_mcp::tools::ListSessionsTool;
use opencode_orchestrator_mcp::types::ListCommandsInput;
use opencode_orchestrator_mcp::types::ListSessionsInput;
use std::sync::Arc;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use support::commands_list_fixture;
use support::sessions_list_fixture;
use support::test_orchestrator_server;

#[tokio::test]
async fn list_sessions_returns_session_ids() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(sessions_list_fixture(&["ses-1", "ses-2"])),
        )
        .mount(&mock)
        .await;

    let tool = ListSessionsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListSessionsInput { limit: None }, &ToolContext::default())
        .await
        .expect("list_sessions should succeed");

    assert_eq!(result.sessions.len(), 2);
    assert_eq!(result.sessions[0].id, "ses-1");
    assert_eq!(result.sessions[1].id, "ses-2");
}

#[tokio::test]
async fn list_sessions_returns_empty_list() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    let tool = ListSessionsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListSessionsInput { limit: None }, &ToolContext::default())
        .await
        .expect("list_sessions should succeed");

    assert!(result.sessions.is_empty());
}

#[tokio::test]
async fn list_commands_returns_available_commands() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(commands_list_fixture()))
        .mount(&mock)
        .await;

    let tool = ListCommandsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListCommandsInput {}, &ToolContext::default())
        .await
        .expect("list_commands should succeed");

    assert_eq!(result.commands.len(), 3);
    assert_eq!(result.commands[0].name, "test");
    assert_eq!(result.commands[1].name, "build");
    assert_eq!(result.commands[2].name, "lint");
}

#[tokio::test]
async fn list_commands_returns_empty_list() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server(&mock);

    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock)
        .await;

    let tool = ListCommandsTool::new(Arc::clone(&server));

    let result = tool
        .call(ListCommandsInput {}, &ToolContext::default())
        .await
        .expect("list_commands should succeed");

    assert!(result.commands.is_empty());
}
