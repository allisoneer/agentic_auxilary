//! Streaming example showing how to subscribe to SSE events.
//!
//! Run with: cargo run --example streaming
//!
//! Requires an OpenCode server running at localhost:4096:
//!   opencode serve

use opencode_rs::ClientBuilder;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::session::CreateSessionRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt::init();

    // Build client
    let client = ClientBuilder::new().build()?;

    // Create session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await?;
    println!("Created session: {}", session.id);

    // Subscribe to session events BEFORE sending prompt
    let mut subscription = client.subscribe_session(&session.id).await?;
    println!("Subscribed to events");

    // Send prompt
    client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Write a haiku about Rust programming".into(),
                }],
            },
        )
        .await?;
    println!("Prompt sent, streaming events...\n");

    // Stream events until session is idle or error
    loop {
        match subscription.recv().await {
            Some(Event::SessionIdle { .. }) => {
                println!("\n[Session completed]");
                break;
            }
            Some(Event::SessionError { properties }) => {
                eprintln!("\n[Session error: {:?}]", properties.error);
                break;
            }
            Some(Event::MessagePartUpdated { properties }) => {
                if let Some(delta) = &properties.delta {
                    print!("{}", delta);
                }
            }
            Some(Event::ServerHeartbeat { .. }) => {
                // Heartbeat received, connection alive
            }
            Some(event) => {
                println!("[Event: {:?}]", event);
            }
            None => {
                println!("[Stream closed]");
                break;
            }
        }
    }

    // Cleanup
    client.sessions().delete(&session.id).await?;
    println!("Session deleted");

    Ok(())
}
