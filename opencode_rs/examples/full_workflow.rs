//! Complete workflow example demonstrating all major SDK capabilities.
//!
//! Run with: cargo run --example full_workflow --features full
//!
//! This example requires an OpenCode server running at localhost:4096:
//!   opencode serve
//!
//! Or use the managed server feature to spawn one automatically.

use opencode_rs::ClientBuilder;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::session::{CreateSessionRequest, UpdateSessionRequest};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt::init();

    println!("=== OpenCode Rust SDK Full Workflow Example ===\n");

    // Step 1: Create client
    println!("1. Creating client...");
    let client = ClientBuilder::new()
        .base_url("http://127.0.0.1:4096")
        .directory(std::env::current_dir()?.to_string_lossy())
        .timeout_secs(300)
        .build()?;

    // Step 2: Check health
    println!("2. Checking server health...");
    let health = client.misc().health().await?;
    println!("   Server healthy: {}", health.healthy);
    if let Some(version) = &health.version {
        println!("   Server version: {}", version);
    }

    // Step 3: Create a session
    println!("\n3. Creating session...");
    let session = client
        .sessions()
        .create(&CreateSessionRequest {
            title: Some("SDK Full Workflow Example".into()),
            ..Default::default()
        })
        .await?;
    println!("   Created session: {}", session.id);
    println!("   Title: {}", session.title);

    // Step 4: Subscribe to events BEFORE sending prompt
    println!("\n4. Subscribing to session events...");
    let mut subscription = client.subscribe_session(&session.id).await?;
    println!("   Subscribed successfully");

    // Step 5: Send a prompt
    println!("\n5. Sending prompt...");
    let prompt_text = "Write a short haiku about programming";
    client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: prompt_text.into(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: None,
                agent: None,
                no_reply: None,
                system: None,
                variant: None,
            },
        )
        .await?;
    println!("   Prompt sent: \"{}\"", prompt_text);

    // Step 6: Stream response events
    println!("\n6. Streaming response...\n");
    let mut response_text = String::new();
    let timeout = Duration::from_secs(60);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            println!("\n   [Timeout reached]");
            break;
        }

        tokio::select! {
            event = subscription.recv() => {
                match event {
                    Some(Event::SessionIdle { .. }) => {
                        println!("\n   [Session completed]");
                        break;
                    }
                    Some(Event::SessionError { properties }) => {
                        eprintln!("\n   [Session error: {:?}]", properties.error);
                        break;
                    }
                    Some(Event::MessagePartUpdated { properties }) => {
                        if let Some(delta) = &properties.delta {
                            print!("{}", delta);
                            response_text.push_str(delta);
                        }
                    }
                    Some(Event::ServerHeartbeat { .. }) => {
                        // Connection alive
                    }
                    Some(_) => {
                        // Other events
                    }
                    None => {
                        println!("\n   [Stream closed]");
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Check periodically
            }
        }
    }

    // Step 7: Update session title
    println!("\n7. Updating session title...");
    let updated = client
        .sessions()
        .update(
            &session.id,
            &UpdateSessionRequest {
                title: Some("Completed Haiku Session".into()),
            },
        )
        .await?;
    println!("   New title: {}", updated.title);

    // Step 8: Get session diff
    println!("\n8. Getting session diff...");
    let diff = client.sessions().diff(&session.id).await?;
    println!("   Files changed: {}", diff.files.len());
    if !diff.diff.is_empty() {
        println!(
            "   Diff preview: {}...",
            &diff.diff[..diff.diff.len().min(100)]
        );
    }

    // Step 9: Get todos
    println!("\n9. Checking todos...");
    let todos = client.sessions().todo(&session.id).await?;
    println!("   Todos: {}", todos.len());
    for todo in &todos {
        println!(
            "   - [{}] {}",
            if todo.completed { "x" } else { " " },
            todo.content
        );
    }

    // Step 10: List messages
    println!("\n10. Listing messages...");
    let messages = client.messages().list(&session.id).await?;
    println!("   Total messages: {}", messages.len());
    for msg in &messages {
        println!("   - {} (parts: {})", msg.role(), msg.parts.len());
    }

    // Step 11: Cleanup - delete session
    println!("\n11. Cleaning up - deleting session...");
    client.sessions().delete(&session.id).await?;
    println!("   Session deleted");

    println!("\n=== Workflow Complete ===");
    println!("\nResponse received:\n{}", response_text);

    Ok(())
}
