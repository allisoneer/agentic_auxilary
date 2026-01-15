//! Model listing example showing how to list available models.

use anthropic_async::{types::ModelListParams, AnthropicConfig, Client};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AnthropicConfig::new();
    let client = Client::with_config(cfg);

    println!("Fetching available models...\n");
    let response = client.models().list(&ModelListParams::default()).await?;

    println!("Available models:");
    for model in &response.data {
        println!("- {} ({})", model.id, model.display_name);
    }

    if let Some(model_id) = response.data.first().map(|m| &m.id) {
        println!("\nFetching details for {model_id}...");
        let model = client.models().get(model_id).await?;
        println!("Created at: {}", model.created_at);
    }

    Ok(())
}
