//! Basic example showing how to create a session and send a prompt.
//!
//! Run with: cargo run --example basic
//!
//! Requires an OpenCode server running at localhost:4096:
//!   opencode serve

use opencode_rs::ClientBuilder;
use opencode_rs::types::message::{PromptPart, PromptRequest};
use opencode_rs::types::session::CreateSessionRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build client connecting to default localhost:4096
    let client = ClientBuilder::new().build()?;

    // Create a new session
    let session = client
        .sessions()
        .create(&CreateSessionRequest::default())
        .await?;
    println!("Created session: {}", session.id);

    // Send a prompt
    client
        .messages()
        .prompt(
            &session.id,
            &PromptRequest {
                parts: vec![PromptPart::Text {
                    text: "Hello OpenCode! What can you help me with?".into(),
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

    println!("Prompt sent successfully!");
    Ok(())
}
