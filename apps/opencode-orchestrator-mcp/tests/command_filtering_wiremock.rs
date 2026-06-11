#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use agentic_config::types::OrchestratorAgentsConfig;
use agentic_config::types::OrchestratorCommandsConfig;
use agentic_config::types::OrchestratorConfig;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use opencode_orchestrator_mcp::tools::ListAgentsTool;
use opencode_orchestrator_mcp::tools::ListCommandsTool;
use opencode_orchestrator_mcp::tools::OrchestratorRunTool;
use opencode_orchestrator_mcp::types::ListAgentsInput;
use opencode_orchestrator_mcp::types::ListCommandsInput;
use opencode_orchestrator_mcp::types::OrchestratorRunInput;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::path_regex;

use support::messages_fixture;
use support::session_fixture;
use support::status_v2_idle;
use support::test_orchestrator_server_with_config;

fn orchestrator_config(
    command_allow: &[&str],
    command_deny: &[&str],
    agent_allow: &[&str],
    agent_deny: &[&str],
) -> OrchestratorConfig {
    OrchestratorConfig {
        commands: OrchestratorCommandsConfig {
            allow: command_allow
                .iter()
                .map(|entry| (*entry).to_string())
                .collect(),
            deny: command_deny
                .iter()
                .map(|entry| (*entry).to_string())
                .collect(),
        },
        agents: OrchestratorAgentsConfig {
            allow: agent_allow
                .iter()
                .map(|entry| (*entry).to_string())
                .collect(),
            deny: agent_deny
                .iter()
                .map(|entry| (*entry).to_string())
                .collect(),
        },
        ..OrchestratorConfig::default()
    }
}

fn command_list(names: &[&str]) -> serde_json::Value {
    json!({
        "location": { "directory": "/tmp" },
        "data": names
            .iter()
            .map(|name| json!({"name": name, "template": name, "description": format!("{name} description")}))
            .collect::<Vec<_>>()
    })
}

fn agent_list(names: &[&str]) -> serde_json::Value {
    json!({
        "location": { "directory": "/tmp" },
        "data": names
            .iter()
            .map(|name| json!({"id": name, "description": format!("{name} description"), "hidden": false}))
            .collect::<Vec<_>>()
    })
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

async fn list_agents_with_config(mock: &MockServer, config: OrchestratorConfig) -> Vec<String> {
    let server = test_orchestrator_server_with_config(mock, config).await;
    let tool = ListAgentsTool::new(Arc::clone(&server));

    tool.call(ListAgentsInput {}, &ToolContext::default())
        .await
        .expect("list_agents should succeed")
        .agents
        .into_iter()
        .map(|agent| agent.name)
        .collect()
}

#[tokio::test]
async fn list_commands_filters_deny_only_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names =
        list_commands_with_config(&mock, orchestrator_config(&[], &["build"], &[], &[])).await;

    assert_eq!(names, vec!["test", "lint"]);
}

#[tokio::test]
async fn list_commands_filters_allow_only_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names =
        list_commands_with_config(&mock, orchestrator_config(&["build"], &[], &[], &[])).await;

    assert_eq!(names, vec!["build"]);
}

#[tokio::test]
async fn list_commands_applies_deny_wins_overlap_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names = list_commands_with_config(
        &mock,
        orchestrator_config(&["test", "build"], &["build"], &[], &[]),
    )
    .await;

    assert_eq!(names, vec!["test"]);
}

#[tokio::test]
async fn list_commands_trims_configured_entries() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/command"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(command_list(&["test", "build", "lint"])),
        )
        .mount(&mock)
        .await;

    let names =
        list_commands_with_config(&mock, orchestrator_config(&[], &[" build "], &[], &[])).await;

    assert_eq!(names, vec!["test", "lint"]);
}

#[tokio::test]
async fn list_commands_matching_is_case_sensitive() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/command"))
        .respond_with(ResponseTemplate::new(200).set_body_json(command_list(&["Build", "build"])))
        .mount(&mock)
        .await;

    let names =
        list_commands_with_config(&mock, orchestrator_config(&[], &["Build"], &[], &[])).await;

    assert_eq!(names, vec!["build"]);
}

#[tokio::test]
async fn blocked_run_command_fails_before_session_resolution_or_dispatch() {
    let mock = MockServer::start().await;
    let server = test_orchestrator_server_with_config(
        &mock,
        orchestrator_config(&[], &["research"], &[], &[]),
    )
    .await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));

    let err = tool
        .call(
            OrchestratorRunInput {
                session_id: None,
                command: Some("research".into()),
                agent: None,
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

#[tokio::test]
async fn list_agents_filters_deny_only_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/agent"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(agent_list(&["Plan", "Bash", "Research"])),
        )
        .mount(&mock)
        .await;

    let names = list_agents_with_config(&mock, orchestrator_config(&[], &[], &[], &["Bash"])).await;

    assert_eq!(names, vec!["Plan", "Research"]);
}

#[tokio::test]
async fn list_agents_filters_allow_only_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/agent"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(agent_list(&["Plan", "Bash", "Research"])),
        )
        .mount(&mock)
        .await;

    let names = list_agents_with_config(&mock, orchestrator_config(&[], &[], &["Bash"], &[])).await;

    assert_eq!(names, vec!["Bash"]);
}

#[tokio::test]
async fn list_agents_applies_deny_wins_overlap_policy() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/agent"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(agent_list(&["Plan", "Bash", "Research"])),
        )
        .mount(&mock)
        .await;

    let names = list_agents_with_config(
        &mock,
        orchestrator_config(&[], &[], &["Plan", "Bash"], &["Bash"]),
    )
    .await;

    assert_eq!(names, vec!["Plan"]);
}

#[tokio::test]
async fn list_agents_trims_configured_entries() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/agent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(agent_list(&["Plan", "Bash"])))
        .mount(&mock)
        .await;

    let names =
        list_agents_with_config(&mock, orchestrator_config(&[], &[], &[], &[" Bash "])).await;

    assert_eq!(names, vec!["Plan"]);
}

#[tokio::test]
async fn list_agents_matching_is_case_sensitive() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/agent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(agent_list(&["Bash", "bash"])))
        .mount(&mock)
        .await;

    let names = list_agents_with_config(&mock, orchestrator_config(&[], &[], &[], &["Bash"])).await;

    assert_eq!(names, vec!["bash"]);
}

#[tokio::test]
async fn blocked_run_agent_fails_before_session_resolution_or_dispatch() {
    let mock = MockServer::start().await;
    let server =
        test_orchestrator_server_with_config(&mock, orchestrator_config(&[], &[], &[], &["Bash"]))
            .await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));

    let err = tool
        .call(
            OrchestratorRunInput {
                session_id: None,
                command: None,
                agent: Some("Bash".into()),
                message: Some("blocked prompt".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("blocked agent should fail before dispatch");

    assert!(matches!(err, ToolError::InvalidInput(_)));
    assert!(err.to_string().contains("orchestrator.agents.deny"));

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

#[tokio::test]
async fn run_prompt_forwards_agent_in_prompt_request() {
    let mock = MockServer::start().await;
    let server =
        test_orchestrator_server_with_config(&mock, orchestrator_config(&[], &[], &["Plan"], &[]))
            .await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));

    Mock::given(method("GET"))
        .and(path("/session/s1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(session_fixture("s1")))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/api/session/.*/wait"))
        .respond_with(ResponseTemplate::new(200).set_body_json(status_v2_idle()))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"/api/session/.*/permission"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/question/request"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/event"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/session/s1/prompt"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data": {"id": "input-1"}})),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/session/s1/context"))
        .respond_with(ResponseTemplate::new(200).set_body_json(messages_fixture("s1", Some("ok"))))
        .mount(&mock)
        .await;

    let result = tokio::time::timeout(
        Duration::from_secs(3),
        tool.call(
            OrchestratorRunInput {
                session_id: Some("s1".into()),
                command: None,
                agent: Some("Plan".into()),
                message: Some("say ok".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        ),
    )
    .await
    .expect("run should complete before timeout")
    .expect("run should succeed");

    assert!(matches!(
        result.status,
        opencode_orchestrator_mcp::types::RunStatus::Completed
    ));
    assert_eq!(result.response.as_deref(), Some("ok"));

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    let prompt_request = requests
        .iter()
        .find(|request| {
            request.method.as_str() == "POST" && request.url.path() == "/api/session/s1/prompt"
        })
        .expect("prompt request should be sent");
    let body: serde_json::Value = serde_json::from_slice(&prompt_request.body)
        .expect("prompt request body should be valid json");
    assert_eq!(body["prompt"]["agents"], json!(["Plan"]));
    assert_eq!(body["prompt"]["text"], json!("say ok"));
}

#[tokio::test]
async fn run_command_and_agent_is_invalid_without_http_calls() {
    let mock = MockServer::start().await;
    let server =
        test_orchestrator_server_with_config(&mock, orchestrator_config(&[], &[], &[], &[])).await;
    let tool = OrchestratorRunTool::new(Arc::clone(&server));

    let err = tool
        .call(
            OrchestratorRunInput {
                session_id: None,
                command: Some("research".into()),
                agent: Some("Plan".into()),
                message: Some("args".into()),
                wait_for_activity: None,
            },
            &ToolContext::default(),
        )
        .await
        .expect_err("command + agent should be rejected");

    assert!(matches!(err, ToolError::InvalidInput(_)));
    assert!(
        err.to_string()
            .contains("agent cannot be provided when command is specified")
    );

    let requests = mock
        .received_requests()
        .await
        .expect("wiremock should capture requests");
    assert!(requests.is_empty(), "unexpected requests: {requests:?}");
}
