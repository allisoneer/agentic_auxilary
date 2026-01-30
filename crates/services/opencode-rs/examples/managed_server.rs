//! Example showing how to spawn and manage an OpenCode server.
//!
//! Run with: cargo run --example managed_server --features server
//!
//! This example requires the `opencode` binary to be installed and in PATH.

use opencode_rs::ClientBuilder;
use opencode_rs::server::{ManagedServer, ServerOptions};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure server options
    let opts = ServerOptions::new()
        .hostname("127.0.0.1")
        .startup_timeout_ms(10000) // 10 second timeout
        .directory(std::env::current_dir()?);

    println!("Starting managed OpenCode server...");

    // Start the server (picks random port)
    let server = ManagedServer::start(opts).await?;
    println!("Server started at: {}", server.url());
    println!("Port: {}", server.port());

    // Create a client connected to the managed server
    let client = ClientBuilder::new()
        .base_url(server.url().to_string())
        .directory(std::env::current_dir()?.to_string_lossy())
        .build()?;

    // Check health
    let health = client.misc().health().await?;
    println!("Server health: {:?}", health);

    // List any existing sessions
    let sessions = client.sessions().list().await?;
    println!("Existing sessions: {}", sessions.len());

    // Keep server running for a bit
    println!("\nServer running for 5 seconds...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Server is automatically stopped when dropped
    println!("Stopping server...");
    server.stop().await?;
    println!("Server stopped.");

    Ok(())
}
