use claudecode::{Client, Event, OutputFormat, SessionConfig};
use tracing::Level;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up detailed logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    println!("Creating client...");
    let client = Client::new().await?;

    let config = SessionConfig::builder("Write a haiku about Rust programming")
        .output_format(OutputFormat::StreamingJson)
        .max_turns(1)
        .verbose(true) // Ensure verbose is on
        .build()?;

    println!("Launching session...");
    let mut session = client.launch(config).await?;
    println!("Session ID: {}", session.id());

    // Process events using type-safe API
    if let Some(mut events) = session.take_event_stream() {
        println!("Got event receiver, waiting for events...");

        let mut event_count = 0;
        while let Some(event) = events.recv().await {
            event_count += 1;
            println!("\n=== Event {event_count} ===");

            match &event {
                Event::System(sys) => {
                    println!("System event!");
                    println!("  Session ID: {}", sys.session_id);
                    if let Some(subtype) = &sys.subtype {
                        println!("  Subtype: {subtype}");
                    }
                    if let Some(model) = &sys.model {
                        println!("  Model: {model}");
                    }
                    if let Some(cwd) = &sys.cwd {
                        println!("  Working directory: {cwd}");
                    }
                }
                Event::Assistant(msg) => {
                    println!("Assistant event!");
                    println!("  Session ID: {}", msg.session_id);
                    println!("  Role: {}", msg.message.role);
                    println!("  Content items: {}", msg.message.content.len());
                    for (i, content) in msg.message.content.iter().enumerate() {
                        match content {
                            claudecode::Content::Text { text } => {
                                println!("  Content[{i}]: Text = {text}");
                            }
                            claudecode::Content::ToolUse { name, id, .. } => {
                                println!("  Content[{i}]: ToolUse name={name}, id={id}");
                            }
                            claudecode::Content::ToolResult {
                                tool_use_id,
                                content,
                            } => {
                                println!(
                                    "  Content[{i}]: ToolResult tool_use_id={tool_use_id}, content={content}"
                                );
                            }
                        }
                    }
                }
                Event::Result(result) => {
                    println!("Result event!");
                    println!("  Session ID: {}", result.session_id);
                    if let Some(res) = &result.result {
                        println!("  Result: {res}");
                    }
                    if let Some(cost) = result.total_cost_usd {
                        println!("  Cost: ${cost:.4}");
                    }
                    if let Some(turns) = result.num_turns {
                        println!("  Turns: {turns}");
                    }
                }
                Event::Error(err) => {
                    println!("Error event!");
                    println!("  Session ID: {}", err.session_id);
                    println!("  Error: {}", err.error);
                }
                Event::Unknown => {
                    println!("Unknown event type (forward compatibility)");
                }
            }
        }

        println!("\nNo more events. Total events received: {event_count}");
    } else {
        println!("No event receiver available!");
    }

    println!("\nWaiting for session to complete...");
    match session.wait().await {
        Ok(result) => {
            println!("\nSession complete!");
            println!("Result: {result:?}");
            if let Some(turns) = result.num_turns {
                println!("Turns used: {turns}");
            }
            if let Some(content) = &result.content {
                println!("Final content: {content}");
            }
        }
        Err(e) => {
            println!("Session error: {e:?}");
        }
    }

    Ok(())
}
