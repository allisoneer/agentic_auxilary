use claudecode::{Client, MCPConfig, MCPServer, OutputFormat, SessionConfig};
use std::collections::HashMap;

#[tokio::test]
async fn test_client_creation() {
    // Skip if claude not available
    if which::which("claude").is_err() {
        eprintln!("Skipping test: claude not found in PATH");
        return;
    }

    let client = Client::new().await;
    assert!(client.is_ok());
}

#[tokio::test]
#[ignore = "requires claude CLI to be installed"]
async fn test_simple_query() {
    if which::which("claude").is_err() {
        return;
    }

    let client = Client::new().await.unwrap();

    let config = SessionConfig::builder("Say 'Hello, Rust!' and nothing else")
        .output_format(OutputFormat::Text)
        .max_turns(1)
        .build()
        .unwrap();

    let result = client.launch_and_wait(config).await.unwrap();
    assert!(!result.is_error);
    assert!(result.content.is_some());
}

#[tokio::test]
#[ignore = "requires claude CLI to be installed"]
async fn test_session_cancellation() {
    if which::which("claude").is_err() {
        return;
    }

    let client = Client::new().await.unwrap();

    let config = SessionConfig::builder("Count to 1000 slowly")
        .output_format(OutputFormat::StreamingJson)
        .build()
        .unwrap();

    let mut session = client.launch(config).await.unwrap();

    // Let it run briefly
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Kill it
    assert!(session.kill().await.is_ok());
}

#[tokio::test]
#[ignore = "requires claude CLI and MCP server to be installed"]
async fn test_mcp_config() {
    if which::which("claude").is_err() {
        return;
    }

    // Check if the MCP calculator server is available
    if std::process::Command::new("npx")
        .args(["@modelcontextprotocol/server-calculator", "--version"])
        .output()
        .is_err()
    {
        eprintln!("Skipping test: MCP calculator server not available");
        return;
    }

    let client = Client::new().await.unwrap();

    // Configure MCP calculator server
    let mut servers = HashMap::new();
    servers.insert(
        "calculator".to_string(),
        MCPServer {
            command: "npx".to_string(),
            args: vec!["@modelcontextprotocol/server-calculator".to_string()],
            env: None,
        },
    );

    let mcp_config = MCPConfig {
        mcp_servers: servers,
    };

    let config = SessionConfig::builder("What is 123 * 456?")
        .mcp_config(mcp_config)
        .output_format(OutputFormat::Json)
        .max_turns(1)
        .build()
        .unwrap();

    let result = client.launch_and_wait(config).await.unwrap();
    assert!(!result.is_error);
    assert!(result.content.is_some());

    // The result should contain the calculation result
    if let Some(content) = result.content {
        assert!(content.contains("56088") || content.contains("56,088"));
    }
}
