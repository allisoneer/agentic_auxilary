//! Simple REST API example using the Universal Tool Framework
//!
//! This example demonstrates:
//! - Basic REST API creation with automatic routing
//! - JSON request/response handling
//! - Error mapping to HTTP status codes
//! - Simple server setup

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use universal_tool_core::prelude::*;
use universal_tool_core::rest::Json;

/// A simple calculator API
#[derive(Clone)]
struct Calculator {
    name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct CalculationResult {
    value: f64,
    operation: String,
}

// The REST prefix is specified at the router level
#[universal_tool_router(rest(prefix = "/api/v1"))]
impl Calculator {
    /// Add two numbers together
    ///
    /// Adds two numbers and returns the sum.
    /// Endpoint: POST /api/v1/add
    #[universal_tool(
        description = "Add two numbers together",
        rest(method = "POST", path = "/add")
    )]
    pub async fn add(&self, a: f64, b: f64) -> Result<CalculationResult, ToolError> {
        Ok(CalculationResult {
            value: a + b,
            operation: format!("{} + {} = {}", a, b, a + b),
        })
    }

    /// Subtract one number from another
    ///
    /// Subtracts b from a.
    /// Endpoint: POST /api/v1/subtract
    #[universal_tool(
        description = "Subtract one number from another",
        rest(method = "POST", path = "/subtract")
    )]
    pub async fn subtract(&self, a: f64, b: f64) -> Result<CalculationResult, ToolError> {
        Ok(CalculationResult {
            value: a - b,
            operation: format!("{} - {} = {}", a, b, a - b),
        })
    }

    /// Multiply two numbers
    ///
    /// Multiplies two numbers together.
    /// Endpoint: POST /api/v1/multiply
    #[universal_tool(
        description = "Multiply two numbers",
        rest(method = "POST", path = "/multiply")
    )]
    pub async fn multiply(&self, a: f64, b: f64) -> Result<CalculationResult, ToolError> {
        Ok(CalculationResult {
            value: a * b,
            operation: format!("{} × {} = {}", a, b, a * b),
        })
    }

    /// Divide one number by another
    ///
    /// Divides a by b with zero-check validation.
    /// Endpoint: POST /api/v1/divide
    #[universal_tool(
        description = "Divide one number by another",
        rest(method = "POST", path = "/divide")
    )]
    pub async fn divide(&self, a: f64, b: f64) -> Result<CalculationResult, ToolError> {
        if b == 0.0 {
            return Err(ToolError::new(
                universal_tool_core::error::ErrorCode::InvalidArgument,
                "Cannot divide by zero",
            ));
        }

        Ok(CalculationResult {
            value: a / b,
            operation: format!("{} ÷ {} = {}", a, b, a / b),
        })
    }

    /// Get information about the calculator
    ///
    /// Returns calculator version information.
    /// Endpoint: GET /api/v1/info
    #[universal_tool(
        description = "Get information about the calculator",
        rest(method = "GET", path = "/info")
    )]
    pub async fn get_info(&self) -> Result<String, ToolError> {
        Ok(format!("Calculator: {}", self.name))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create the calculator instance wrapped in Arc for thread-safe sharing
    let calculator = Arc::new(Calculator {
        name: "UTF Calculator v1.0".to_string(),
    });

    // Create the REST router using the associated function syntax
    // Note: create_rest_router is an associated function, not a method
    // It takes Arc<Self> to ensure thread-safe sharing across handlers
    let app = Calculator::create_rest_router(calculator.clone());

    // Add user-defined endpoints
    let app = app
        .route(
            "/health",
            universal_tool_core::rest::routing::get(health_check),
        )
        .route(
            "/openapi.json",
            universal_tool_core::rest::routing::get({
                let calculator = calculator.clone();
                move || async move {
                    // For now, OpenAPI returns a string message
                    // TODO: Enable the 'openapi' feature for full OpenAPI support
                    calculator.get_openapi_spec()
                }
            }),
        );

    // Start the server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("🚀 REST API server running on http://127.0.0.1:3000");
    println!("📍 Try these endpoints:");
    println!("   GET  http://127.0.0.1:3000/health");
    println!("   GET  http://127.0.0.1:3000/openapi.json");
    println!("   POST http://127.0.0.1:3000/add");
    println!("   POST http://127.0.0.1:3000/subtract");
    println!("   POST http://127.0.0.1:3000/multiply");
    println!("   POST http://127.0.0.1:3000/divide");
    println!("\nExample request:");
    println!("  curl -X POST http://127.0.0.1:3000/add \\");
    println!("    -H \"Content-Type: application/json\" \\");
    println!("    -d '{{\"a\": 10, \"b\": 5}}'");

    universal_tool_core::rest::axum::serve(listener, app)
        .await
        .unwrap();

    Ok(())
}

/// Health check endpoint (user-defined)
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "calculator-api"
    }))
}
