//! Simple CLI example demonstrating UTF's CLI generation capabilities
//!
//! This example shows how UTF generates CLI methods without controlling your application.

use serde::{Deserialize, Serialize};
use universal_tool_core::prelude::*;

/// A simple math tools implementation
struct MathTools;

impl MathTools {
    fn new() -> Self {
        Self
    }
}

#[universal_tool_router(cli(name = "mathtools", description = "Simple math operations"))]
impl MathTools {
    /// Add two numbers
    #[universal_tool(description = "Add two numbers together", cli(name = "add"))]
    async fn add(&self, a: i32, b: i32) -> Result<i32, ToolError> {
        Ok(a + b)
    }

    /// Multiply two numbers
    #[universal_tool(description = "Multiply two numbers", cli(name = "multiply"))]
    async fn multiply(&self, x: f64, y: f64) -> Result<f64, ToolError> {
        Ok(x * y)
    }

    /// Concatenate strings
    #[universal_tool(
        description = "Concatenate two strings with an optional separator",
        cli(name = "concat")
    )]
    async fn concat_strings(
        &self,
        first: String,
        second: String,
        #[universal_tool_param(
            default = " ",
            description = "Separator between strings"
        )]
        separator: Option<String>,
    ) -> Result<String, ToolError> {
        let sep = separator.unwrap_or_else(|| " ".to_string());
        Ok(format!("{}{}{}", first, sep, second))
    }

    /// Check if a number is even
    #[universal_tool(
        description = "Check if a number is even",
        cli(name = "is-even", alias = "even", alias = "e")
    )]
    async fn is_even(&self, number: i32) -> Result<bool, ToolError> {
        Ok(number % 2 == 0)
    }

    /// Secret debug function
    #[universal_tool(
        description = "Secret debug function (hidden from help)",
        cli(name = "debug", hidden = true)
    )]
    async fn debug_info(&self, verbose: bool) -> Result<String, ToolError> {
        if verbose {
            Ok("Debug mode: VERBOSE".to_string())
        } else {
            Ok("Debug mode: NORMAL".to_string())
        }
    }
}

/// Result for the analyze operation
#[derive(Debug, Serialize, Deserialize)]
struct AnalysisResult {
    word_count: usize,
    char_count: usize,
    has_uppercase: bool,
}

/// Text analysis tools
struct TextTools;

impl TextTools {
    fn new() -> Self {
        Self
    }
}

#[universal_tool_router(cli(name = "texttools", description = "Text analysis and manipulation"))]
impl TextTools {
    /// Analyze text properties
    #[universal_tool(
        description = "Analyze text to get various properties",
        cli(name = "analyze")
    )]
    async fn analyze_text(
        &self,
        text: String,
        #[universal_tool_param(
            short = 'd',
            long = "detailed",
            description = "Show detailed analysis"
        )]
        detailed: bool,
    ) -> Result<AnalysisResult, ToolError> {
        let result = AnalysisResult {
            word_count: text.split_whitespace().count(),
            char_count: text.chars().count(),
            has_uppercase: text.chars().any(|c| c.is_uppercase()),
        };

        if detailed {
            eprintln!("Detailed analysis:");
            eprintln!("- Words: {}", result.word_count);
            eprintln!("- Characters: {}", result.char_count);
            eprintln!("- Has uppercase: {}", result.has_uppercase);
        }

        Ok(result)
    }

    /// Reverse a string
    #[universal_tool(
        description = "Reverse the characters in a string",
        cli(name = "reverse")
    )]
    async fn reverse_string(&self, input: String) -> Result<String, ToolError> {
        Ok(input.chars().rev().collect())
    }

    /// Format text with a prefix
    #[universal_tool(
        description = "Format text with a configurable prefix",
        cli(name = "format")
    )]
    async fn format_text(
        &self,
        text: String,
        #[universal_tool_param(
            env = "TEXT_PREFIX",
            default = ">>>",
            description = "Prefix to add before text (can be set via TEXT_PREFIX env var)"
        )]
        prefix: Option<String>,
    ) -> Result<String, ToolError> {
        let prefix = prefix.unwrap_or_else(|| ">>>".to_string());
        Ok(format!("{} {}", prefix, text))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // UTF provides building blocks - you control the application structure

    // Create instances of our tool collections
    let math_tools = MathTools::new();
    let text_tools = TextTools::new();

    // Build a top-level CLI that combines both tool sets
    let app = universal_tool_core::cli::clap::Command::new("example-cli")
        .about("Example CLI demonstrating UTF's code generation capabilities")
        .version("1.0")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            math_tools
                .create_cli_command()
                .name("math")
                .about("Math operations"),
        )
        .subcommand(
            text_tools
                .create_cli_command()
                .name("text")
                .about("Text analysis and manipulation tools"),
        );

    // Parse the command line arguments
    let matches = app.get_matches();

    // Route to the appropriate tool set based on the subcommand
    match matches.subcommand() {
        Some(("math", sub_matches)) => {
            // The math_tools CLI is already configured, we just need to handle its subcommands
            if let Err(e) = math_tools.execute_cli(sub_matches.clone()).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(("text", sub_matches)) => {
            // The text_tools CLI is already configured, we just need to handle its subcommands
            if let Err(e) = text_tools.execute_cli(sub_matches.clone()).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        _ => unreachable!("clap should ensure we have a valid subcommand"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_math_operations() {
        let tools = MathTools::new();

        // Test add
        assert_eq!(tools.add(5, 3).await.unwrap(), 8);

        // Test multiply
        assert_eq!(tools.multiply(2.5, 4.0).await.unwrap(), 10.0);

        // Test is_even
        assert_eq!(tools.is_even(4).await.unwrap(), true);
        assert_eq!(tools.is_even(5).await.unwrap(), false);
    }

    #[tokio::test]
    async fn test_string_operations() {
        let tools = MathTools::new();

        // Test concat with separator
        let result = tools
            .concat_strings(
                "Hello".to_string(),
                "World".to_string(),
                Some(", ".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(result, "Hello, World");

        // Test concat without separator
        let result = tools
            .concat_strings("Hello".to_string(), "World".to_string(), None)
            .await
            .unwrap();
        assert_eq!(result, "Hello World");
    }

    #[tokio::test]
    async fn test_text_analysis() {
        let tools = TextTools::new();

        // Test analyze
        let result = tools
            .analyze_text("Hello World".to_string(), false)
            .await
            .unwrap();
        assert_eq!(result.word_count, 2);
        assert_eq!(result.char_count, 11);
        assert_eq!(result.has_uppercase, true);

        // Test reverse
        let reversed = tools.reverse_string("Hello".to_string()).await.unwrap();
        assert_eq!(reversed, "olleH");
    }
}
