//! Advanced MCP server example using the Universal Tool Framework
//!
//! This example demonstrates:
//! - Creating an MCP server with UTF-generated methods
//! - Tool discovery via get_mcp_tools()
//! - Method dispatch via handle_mcp_call()
//! - Integration with rmcp's ServerHandler trait
//! - Progress reporting for long-running operations
//! - Cancellation token support
//! - MCP tool annotations (read_only, destructive, idempotent)
//! - Error handling with MCP error codes

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use universal_tool_core::mcp::{ServiceExt, stdio};
use universal_tool_core::prelude::*;

/// A simple text processing tool suite
#[derive(Clone)]
struct TextTools;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WordCountResult {
    words: usize,
    lines: usize,
    characters: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TransformResult {
    original: String,
    transformed: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ProcessingResult {
    #[schemars(description = "Number of items processed")]
    processed_count: usize,
    #[schemars(description = "Processing duration in milliseconds")]
    duration_ms: u64,
    #[schemars(description = "Whether the operation was cancelled")]
    cancelled: bool,
}

// MCP-specific attributes like read_only and destructive can be added to methods
#[universal_tool_router(mcp(name = "text-tools", version = "1.0.0"))]
impl TextTools {
    /// Count words, lines, and characters in text
    ///
    /// Analyzes text and returns word, line, and character counts.
    #[universal_tool(description = "Count words, lines, and characters in text")]
    pub async fn analyze_text(&self, text: String) -> Result<WordCountResult, ToolError> {
        let words = text.split_whitespace().count();
        let lines = text.lines().count();
        let characters = text.chars().count();

        Ok(WordCountResult {
            words,
            lines,
            characters,
        })
    }

    /// Convert text to uppercase
    ///
    /// Transforms text to uppercase.
    #[universal_tool(description = "Convert text to uppercase")]
    pub async fn to_uppercase(&self, text: String) -> Result<TransformResult, ToolError> {
        Ok(TransformResult {
            original: text.clone(),
            transformed: text.to_uppercase(),
        })
    }

    /// Convert text to lowercase
    ///
    /// Transforms text to lowercase.
    #[universal_tool(description = "Convert text to lowercase")]
    pub async fn to_lowercase(&self, text: String) -> Result<TransformResult, ToolError> {
        Ok(TransformResult {
            original: text.clone(),
            transformed: text.to_lowercase(),
        })
    }

    /// Reverse text
    ///
    /// Reverses the characters in text.
    #[universal_tool(description = "Reverse text")]
    pub async fn reverse_text(&self, text: String) -> Result<TransformResult, ToolError> {
        Ok(TransformResult {
            original: text.clone(),
            transformed: text.chars().rev().collect(),
        })
    }

    /// Count characters (synchronous method)
    ///
    /// Counts the number of characters in text.
    /// This is a synchronous method - UTF handles both sync and async methods.
    #[universal_tool(description = "Count characters in text")]
    pub async fn count_chars(&self, text: String) -> Result<usize, ToolError> {
        Ok(text.chars().count())
    }

    /// Extract summary from text
    ///
    /// Extracts a summary from text (first N words).
    /// MCP hints: read_only, idempotent
    #[universal_tool(
        description = "Extract summary from text",
        mcp(read_only = true, idempotent = true)
    )]
    pub async fn summarize(
        &self,
        text: String,
        max_words: Option<usize>,
    ) -> Result<String, ToolError> {
        let max_words = max_words.unwrap_or(50);
        let words: Vec<&str> = text.split_whitespace().collect();

        if words.len() <= max_words {
            Ok(text)
        } else {
            Ok(words[..max_words].join(" ") + "...")
        }
    }

    /// Clear all text (destructive operation)
    ///
    /// Clears all text - this is a destructive operation.
    /// MCP hint: destructive
    #[universal_tool(
        description = "Clear all text (destructive operation)",
        mcp(destructive = true)
    )]
    pub async fn clear_text(&self, confirm: String) -> Result<String, ToolError> {
        if confirm.to_lowercase() != "yes" {
            return Err(ToolError::new(
                ErrorCode::InvalidArgument,
                "Confirmation required: please pass 'yes' to confirm",
            ));
        }
        Ok("Text cleared successfully".to_string())
    }

    /// Process large dataset (progress reporting not supported in this example)
    ///
    /// Processes a large dataset with configurable delay.
    #[universal_tool(description = "Process large dataset")]
    pub async fn process_large_dataset(
        &self,
        #[universal_tool_param(description = "Number of items to process")] item_count: usize,
        #[universal_tool_param(description = "Processing delay per item in ms")] delay_ms: Option<
            u64,
        >,
    ) -> Result<ProcessingResult, ToolError> {
        let delay_ms = delay_ms.unwrap_or(100);
        let start = std::time::Instant::now();
        let mut processed = 0;

        for _i in 0..item_count {
            // Simulate processing
            sleep(Duration::from_millis(delay_ms)).await;
            processed += 1;
        }

        Ok(ProcessingResult {
            processed_count: processed,
            duration_ms: start.elapsed().as_millis() as u64,
            cancelled: false,
        })
    }

    /// Validate input with detailed error codes
    ///
    /// Validates input text according to rules.
    /// MCP hint: read_only
    #[universal_tool(
        description = "Validate input with detailed error codes",
        mcp(read_only = true)
    )]
    pub async fn validate_text(
        &self,
        text: String,
        #[universal_tool_param(description = "Minimum length required")] min_length: Option<usize>,
        #[universal_tool_param(description = "Maximum length allowed")] max_length: Option<usize>,
        #[universal_tool_param(description = "Required pattern (substring)")]
        required_pattern: Option<String>,
    ) -> Result<bool, ToolError> {
        // Check minimum length
        if let Some(min) = min_length
            && text.len() < min
        {
            return Err(ToolError::new(
                ErrorCode::InvalidArgument,
                format!(
                    "Text too short: {} chars, minimum {} required",
                    text.len(),
                    min
                ),
            )
            .with_detail(
                "help",
                "Provide longer text to meet the minimum length requirement",
            ));
        }

        // Check maximum length
        if let Some(max) = max_length
            && text.len() > max
        {
            return Err(ToolError::new(
                ErrorCode::InvalidArgument,
                format!(
                    "Text too long: {} chars, maximum {} allowed",
                    text.len(),
                    max
                ),
            )
            .with_detail(
                "help",
                "Shorten the text to meet the maximum length requirement",
            ));
        }

        // Check required pattern
        if let Some(pattern) = required_pattern
            && !text.contains(&pattern)
        {
            return Err(ToolError::new(
                ErrorCode::InvalidArgument,
                format!("Required pattern '{pattern}' not found in text"),
            )
            .with_detail("help", "The text must contain the specified pattern"));
        }

        Ok(true)
    }
}

/// Our MCP server implementation
struct TextToolsServer {
    tools: Arc<TextTools>,
}

// Use the UTF-provided macro to implement ServerHandler
universal_tool_core::implement_mcp_server!(TextToolsServer, tools);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the text tools instance
    let tools = Arc::new(TextTools);

    // Create the MCP server
    let server = TextToolsServer { tools };

    // Print startup information
    eprintln!("🚀 Starting MCP Text Tools Server");
    eprintln!("📍 Communicating via stdio");
    eprintln!();
    eprintln!("Available tools:");
    eprintln!("  - analyze_text: Count words, lines, and characters");
    eprintln!("  - to_uppercase: Convert text to uppercase");
    eprintln!("  - to_lowercase: Convert text to lowercase");
    eprintln!("  - reverse_text: Reverse text characters");
    eprintln!("  - summarize: Extract text summary (read-only, idempotent)");
    eprintln!("  - clear_text: Clear all text (destructive!)");
    eprintln!("  - process_large_dataset: Process with progress & cancellation");
    eprintln!("  - validate_text: Validate text with detailed errors");
    eprintln!();
    eprintln!("Connect with: mcp-client stdio -- cargo run --example 05-mcp-basic");

    // Create stdio transport
    let transport = stdio();

    // Run the MCP server and wait for completion
    let service = match server.serve(transport).await {
        Ok(service) => service,
        Err(e) => {
            eprintln!("MCP server error: {}", e);

            // Add targeted hints for common handshake issues
            let msg = format!("{e}");
            if msg.contains("ExpectedInitializeRequest") || msg.contains("expect initialized request") {
                eprintln!("Hint: Client must send 'initialize' request first.");
            }
            if msg.contains("ExpectedInitializedNotification") || msg.contains("initialize notification") {
                eprintln!("Hint: Client must send 'notifications/initialized' after receiving InitializeResult.");
            }
            return Err(Box::new(e) as Box<dyn std::error::Error>);
        }
    };

    // Wait for the service to complete
    service.waiting().await?;
    Ok(())
}
