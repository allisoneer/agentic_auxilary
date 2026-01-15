//! This is the version that uses the macro - currently broken due to Rust 1.88 bug
//! See main.rs for the working version without macros

use serde::{Deserialize, Serialize};
use universal_tool_core::prelude::*;

/// A simple math tools implementation
struct MathTools;

#[universal_tool_router]
impl MathTools {
    fn new() -> Self {
        Self
    }
    
    /// Add two numbers together
    pub async fn add(&self, a: i32, b: i32) -> Result<i32, ToolError> {
        Ok(a + b)
    }
    
    /// Multiply two numbers
    pub async fn multiply(&self, x: f64, y: f64) -> Result<f64, ToolError> {
        Ok(x * y)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let math_tools = MathTools::new();
    
    // This would work if the macro compiled:
    // let app = math_tools.create_cli_command();
    // let matches = app.get_matches();
    // math_tools.execute_cli(matches).await?;
    
    Ok(())
}