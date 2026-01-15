use claudecode::{Client, Event, OutputFormat, SessionConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let client = Client::new().await?;

    let config = SessionConfig::builder("Write a haiku about Rust programming")
        .output_format(OutputFormat::StreamingJson)
        .build()?;

    let mut session = client.launch(config).await?;
    println!("Session ID: {}", session.id());

    // Process events using the new type-safe API
    if let Some(mut events) = session.take_event_stream() {
        println!("Starting to receive events...");
        while let Some(event) = events.recv().await {
            match event {
                Event::Assistant(msg) => {
                    // Direct access to message content
                    for content in &msg.message.content {
                        match content {
                            claudecode::Content::Text { text } => print!("{text}"),
                            claudecode::Content::ToolUse { name, .. } => {
                                println!("[Tool use: {name}]")
                            }
                            claudecode::Content::ToolResult { .. } => {}
                        }
                    }
                }
                Event::Result(result) => {
                    // Handle completion
                    if let Some(cost) = result.total_cost_usd {
                        println!("\n\nTotal cost: ${cost:.4}");
                    }
                }
                Event::Error(err) => {
                    eprintln!("Error: {}", err.error);
                }
                Event::System(sys) => {
                    // Log system events if needed
                    if sys.subtype.as_deref() == Some("init") {
                        println!("Initialized with model: {:?}", sys.model);
                    }
                }
                Event::Unknown => {
                    // Forward compatibility
                }
            }
        }
        println!("No more events received");
    }

    let result = session.wait().await?;
    println!("\nSession complete!");
    if let Some(turns) = result.num_turns {
        println!("Turns used: {turns}");
    }

    Ok(())
}
