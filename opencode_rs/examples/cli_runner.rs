//! Example showing how to use the CLI runner fallback.
//!
//! Run with: cargo run --example cli_runner --features cli
//!
//! This example requires the `opencode` binary to be installed and in PATH.

use opencode_rs::cli::{CliRunner, RunOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure CLI options
    let opts = RunOptions::new()
        .model("anthropic/claude-3-5-sonnet")
        .title("CLI Runner Example")
        .directory(std::env::current_dir()?);

    println!("Starting CLI runner...");
    println!("Sending prompt: 'What is 2 + 2?'");

    // Start the CLI runner
    let mut runner = CliRunner::start("What is 2 + 2? Reply with just the number.", opts).await?;

    println!("\nStreaming events:\n");

    // Stream events
    let mut text_buffer = String::new();
    while let Some(event) = runner.recv().await {
        match event.r#type.as_str() {
            "step_start" => {
                println!("[Step started]");
            }
            "text" => {
                if let Some(text) = event.text() {
                    print!("{}", text);
                    text_buffer.push_str(text);
                }
            }
            "tool_use" => {
                if let Some(tool) = event.data.get("tool").and_then(|t| t.as_str()) {
                    println!("[Tool: {}]", tool);
                }
            }
            "step_finish" => {
                println!("\n[Step finished]");
            }
            "error" => {
                if let Some(msg) = event.data.get("message").and_then(|m| m.as_str()) {
                    eprintln!("[Error: {}]", msg);
                }
            }
            _ => {
                // Handle other event types
            }
        }
    }

    println!("\n\nFinal response: {}", text_buffer.trim());
    println!("Done!");

    Ok(())
}
