#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_config::types::OrchestratorCommandsConfig;
use agentic_config::types::OrchestratorConfig;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use opencode_orchestrator_mcp::tools::ListCommandsTool;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::types::ListCommandsInput;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use serde_json::json;
use std::sync::Arc;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use support::test_orchestrator_server_with_config;

fn orchestrator_config(allow: &[&str], deny: &[&str]) -> OrchestratorConfig {
    OrchestratorConfig {
        commands: OrchestratorCommandsConfig {
            allow: allow.iter().map(|entry| (*entry).to_string()).collect(),
            deny: deny.iter().map(|entry| (*entry).to_string()).collect(),
        },
        ..OrchestratorConfig::default()
    }
}

fn command_list(names: &[&str]) -> serde_json::Value {
    json!(
        names
            .iter()
            .map(|name| json!({"name": name, "description": format!("{name} description")}))
            .collect::<Vec<_>>()
    )
}

async fn list_commands_with_config(mock: &MockServer, config: OrchestratorConfig) -> Vec<String> {
    let server = test_orchestrator_server_with_config(mock, config).await;
    let tool = ListCommandsTool::new(Arc::clone(&server));

    tool.call(ListCommandsInput {}, &ToolContext::default())
        .await
        .expect("list_commands should succeed")
        .commands
        .into_iter()
        .map(|command| command.name)
        .collect()
}

#[tokio::test]
async fn list_commands_filters_deny_only_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names = list_commands_with_config(&mock, orchestrator_config(&[], &["build"])).await;

    assert_eq!(names, vec!["test", "lint"]);
}

#[tokio::test]
async fn list_commands_filters_allow_only_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names = list_commands_with_config(&mock, orchestrator_config(&["build"], &[])).await;

    assert_eq!(names, vec!["build"]);
}

#[tokio::test]
async fn list_commands_applies_deny_wins_overlap_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names =
        list_commands_with_config(&mock, orchestrator_config(&["test", "build"], &["build"])).await;

    assert_eq!(names, vec!["test"]);
}

#[tokio::test]
async fn list_commands_trims_configured_entries() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names = list_commands_with_config(&mock, orchestrator_config(&[], &[" build "])).await;

    assert_eq!(names, vec!["test", "lint"]);
}

#[tokio::test]
async fn list_commands_matching_is_case_sensitive() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(command_list(&["Build", "build"])))
        .mount(&mock)
        .await;

    let names = list_commands_with_config(&mock, orchestrator_config(&[], &["Build"])).await;

    assert_eq!(names, vec!["build"]);
}

#[tokio::test]
async fn blocked_run_command_fails_before_session_resolution_or_dispatch() {
    let mock = MockServer::start().await;
    let server =
        test_orchestrator_server_with_config(&mock, orchestrator_config(&[], &["research"])).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));

    let err = tool
        .call(
            OrchestratorRunInput {
                session_id: None,
                command: Some("research".into()),
                message: Some("blocked arguments".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("blocked command should fail before dispatch");

    assert!(matches!(err, ToolError::InvalidInput(_)));
    assert!(err.to_string().contains("orchestrator.commands.deny"));

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    assert!(
        !requests
            .iter()
            .any(|request| request.url.path().starts_with("/session")),
        "unexpected session request(s): {:?}",
        requests
            .iter()
            .map(|request| request.url.path().to_string())
            .collect::<Vec<_>>()
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.url.path() == "/event"),
        "unexpected event subscription request"
    );
}
