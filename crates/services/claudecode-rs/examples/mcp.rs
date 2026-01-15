use claudecode::{Client, MCPConfig, MCPServer, SessionConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new().await?;

    // Configure MCP servers
    let mut servers = HashMap::new();
    servers.insert(
        "calculator".to_string(),
        MCPServer::stdio(
            "npx",
            vec!["@modelcontextprotocol/server-calculator".to_string()],
        ),
    );

    let mcp_config = MCPConfig {
        mcp_servers: servers,
    };

    let config = SessionConfig::builder("What is 42 * 17?")
        .mcp_config(mcp_config)
        .build()?;

    let result = client.launch_and_wait(config).await?;

    if let Some(content) = result.content {
        println!("Result: {content}");
    }

    Ok(())
}
