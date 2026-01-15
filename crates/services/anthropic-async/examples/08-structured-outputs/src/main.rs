//! Structured outputs example showing JSON schema constrained responses.
//!
//! This example demonstrates:
//! - Using structured outputs beta feature
//! - Defining a JSON schema for response format
//! - Parsing the structured response back to a Rust type
//!
//! Note: Structured outputs requires the beta header to be enabled.

use anthropic_async::{
    config::BetaFeature,
    types::{content::*, messages::*},
    AnthropicConfig, Client,
};
use serde::{Deserialize, Serialize};

/// The expected structure of the AI's response
#[derive(Debug, Serialize, Deserialize)]
struct RecipeResponse {
    name: String,
    ingredients: Vec<Ingredient>,
    steps: Vec<String>,
    prep_time_minutes: u32,
    cook_time_minutes: u32,
    servings: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Ingredient {
    name: String,
    amount: String,
    unit: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure client with structured outputs beta feature
    let cfg = AnthropicConfig::new()
        .with_beta_features([BetaFeature::StructuredOutputsLatest]);

    let client = Client::with_config(cfg);

    // Define the JSON schema for the response
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Name of the recipe"
            },
            "ingredients": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "amount": { "type": "string" },
                        "unit": { "type": "string" }
                    },
                    "required": ["name", "amount", "unit"]
                }
            },
            "steps": {
                "type": "array",
                "items": { "type": "string" }
            },
            "prep_time_minutes": { "type": "integer" },
            "cook_time_minutes": { "type": "integer" },
            "servings": { "type": "integer" }
        },
        "required": ["name", "ingredients", "steps", "prep_time_minutes", "cook_time_minutes", "servings"]
    });

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 1024,
        system: Some("You are a helpful cooking assistant that provides recipes.".into()),
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "Give me a simple pasta recipe with tomato sauce.".into(),
        }],
        temperature: Some(0.5),
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
        stream: None,
        output_format: Some(OutputFormat::JsonSchema { schema }),
    };

    println!("Requesting structured recipe from Claude...\n");
    let response = client.messages().create(req).await?;

    // Extract and parse the JSON response
    for block in &response.content {
        if let ContentBlock::Text { text } = block {
            // Parse the JSON response into our typed struct
            match serde_json::from_str::<RecipeResponse>(text) {
                Ok(recipe) => {
                    println!("Recipe: {}", recipe.name);
                    println!("Servings: {}", recipe.servings);
                    println!(
                        "Time: {} min prep + {} min cook\n",
                        recipe.prep_time_minutes, recipe.cook_time_minutes
                    );

                    println!("Ingredients:");
                    for ing in &recipe.ingredients {
                        println!("  - {} {} {}", ing.amount, ing.unit, ing.name);
                    }

                    println!("\nSteps:");
                    for (i, step) in recipe.steps.iter().enumerate() {
                        println!("  {}. {}", i + 1, step);
                    }
                }
                Err(e) => {
                    println!("Failed to parse response as recipe: {e}");
                    println!("Raw response: {text}");
                }
            }
        }
    }

    if let Some(usage) = &response.usage {
        println!("\nToken usage:");
        println!("  Input: {:?}", usage.input_tokens);
        println!("  Output: {:?}", usage.output_tokens);
    }

    Ok(())
}
