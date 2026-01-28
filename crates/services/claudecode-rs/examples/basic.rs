use claudecode::{Client, Model, SessionConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create client
    let client = Client::new().await?;

    // Simple text query using the new Haiku model
    let config = SessionConfig::builder("What is the capital of France?")
        .model(Model::Haiku)
        .build()?;

    println!("Asking Claude...");
    let result = client.launch_and_wait(config).await?;

    if let Some(content) = result.content {
        println!("Claude says: {content}");
    }

    if let Some(cost) = result.total_cost_usd {
        println!("Cost: ${cost:.4}");
    }

    Ok(())
}
