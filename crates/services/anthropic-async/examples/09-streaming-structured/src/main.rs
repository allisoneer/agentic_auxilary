//! Streaming structured outputs example combining real-time responses with JSON schema constraints.
//!
//! This example demonstrates:
//! - Streaming responses with structured output format
//! - Watching JSON being built incrementally
//! - Using the Accumulator to get the final complete JSON
//! - Parsing the structured response back to a Rust type
//!
//! Note: Structured outputs requires the beta header to be enabled.

use anthropic_async::{
    config::BetaFeature,
    streaming::{Accumulator, ContentBlockDeltaData, Event},
    types::{content::*, messages::*},
    AnthropicConfig, Client,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// The expected structure of the AI's response
#[derive(Debug, Serialize, Deserialize)]
struct MovieReview {
    title: String,
    year: u32,
    rating: f32,
    summary: String,
    pros: Vec<String>,
    cons: Vec<String>,
    recommendation: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure client with structured outputs beta feature
    let cfg =
        AnthropicConfig::new().with_beta_features([BetaFeature::StructuredOutputsLatest]);

    let client = Client::with_config(cfg);

    // Define the JSON schema for the response
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "year": { "type": "integer" },
            "rating": { "type": "number", "minimum": 0, "maximum": 10 },
            "summary": { "type": "string" },
            "pros": {
                "type": "array",
                "items": { "type": "string" }
            },
            "cons": {
                "type": "array",
                "items": { "type": "string" }
            },
            "recommendation": { "type": "string" }
        },
        "required": ["title", "year", "rating", "summary", "pros", "cons", "recommendation"]
    });

    let req = MessagesCreateRequest {
        model: "claude-3-5-sonnet".into(),
        max_tokens: 1024,
        system: Some("You are a movie critic. Provide detailed reviews in the requested format."
            .into()),
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: "Review the movie 'Inception' (2010) directed by Christopher Nolan.".into(),
        }],
        temperature: Some(0.7),
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
        stream: None, // Will be set by create_stream()
        output_format: Some(OutputFormat::JsonSchema { schema }),
    };

    println!("Streaming structured movie review from Claude...\n");
    println!("=== JSON being built ===\n");

    let mut stream = client.messages().create_stream(req).await?;
    let mut accumulator = Accumulator::new();

    while let Some(event_result) = stream.next().await {
        let event = event_result?;

        // Apply to accumulator
        if let Some(response) = accumulator.apply(&event)? {
            // Message complete - parse the final JSON
            println!("\n\n=== Parsed Review ===\n");

            for block in &response.content {
                if let ContentBlock::Text { text } = block {
                    match serde_json::from_str::<MovieReview>(text) {
                        Ok(review) => {
                            println!("{} ({})", review.title, review.year);
                            println!("Rating: {}/10\n", review.rating);
                            println!("Summary: {}\n", review.summary);

                            println!("Pros:");
                            for pro in &review.pros {
                                println!("  + {pro}");
                            }

                            println!("\nCons:");
                            for con in &review.cons {
                                println!("  - {con}");
                            }

                            println!("\nRecommendation: {}", review.recommendation);
                        }
                        Err(e) => {
                            println!("Failed to parse: {e}");
                            println!("Raw: {text}");
                        }
                    }
                }
            }
            break;
        }

        // Show streaming progress
        if let Event::ContentBlockDelta { delta, .. } = event {
            if let ContentBlockDeltaData::TextDelta { text } = delta {
                print!("{text}");
                io::stdout().flush()?;
            }
        }
    }

    Ok(())
}
