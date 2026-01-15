//! Integration tests for MCP server functionality.
//!
//! These tests verify the RegistryServer logic without requiring
//! a full MCP transport layer.

use agentic_tools_core::fmt::TextOptions;
use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use agentic_tools_mcp::{OutputMode, RegistryServer};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =============================================================================
// Test Tool Definitions
// =============================================================================

#[derive(Clone)]
struct GreetTool;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct GreetInput {
    /// Name to greet
    name: String,
    /// Include exclamation mark
    #[serde(default)]
    excited: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct GreetOutput {
    greeting: String,
}

impl Tool for GreetTool {
    type Input = GreetInput;
    type Output = GreetOutput;
    const NAME: &'static str = "greet";
    const DESCRIPTION: &'static str = "Greet someone by name";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            let greeting = if input.excited {
                format!("Hello, {}!", input.name)
            } else {
                format!("Hello, {}", input.name)
            };
            Ok(GreetOutput { greeting })
        })
    }
}

#[derive(Clone)]
struct CalculateTool;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct CalculateInput {
    a: i32,
    b: i32,
    operation: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct CalculateOutput {
    result: i32,
}

impl Tool for CalculateTool {
    type Input = CalculateInput;
    type Output = CalculateOutput;
    const NAME: &'static str = "calculate";
    const DESCRIPTION: &'static str = "Perform arithmetic calculation";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            let result = match input.operation.as_str() {
                "add" => input.a + input.b,
                "sub" => input.a - input.b,
                "mul" => input.a * input.b,
                _ => return Err(ToolError::invalid_input("Unknown operation")),
            };
            Ok(CalculateOutput { result })
        })
    }
}

// =============================================================================
// RegistryServer Tests
// =============================================================================

fn create_test_registry() -> Arc<ToolRegistry> {
    Arc::new(
        ToolRegistry::builder()
            .register::<GreetTool, ()>(GreetTool)
            .register::<CalculateTool, ()>(CalculateTool)
            .finish(),
    )
}

#[test]
fn test_server_creation() {
    let registry = create_test_registry();
    let server = RegistryServer::new(registry).with_info("test-server", "1.0.0");

    assert_eq!(server.name(), "test-server");
    assert_eq!(server.version(), "1.0.0");
}

#[test]
fn test_server_with_allowlist() {
    let registry = create_test_registry();
    let server = RegistryServer::new(registry).with_allowlist(["greet".to_string()]);

    // Verify server only exposes allowed tools
    let names = server.effective_tool_names();
    assert_eq!(names, vec!["greet".to_string()]);
}

#[test]
fn test_server_output_modes() {
    let registry = create_test_registry();

    // JSON mode
    let server_json = RegistryServer::new(registry.clone()).with_output_mode(OutputMode::Json);
    assert!(matches!(server_json.output_mode(), OutputMode::Json));

    // Text mode
    let server_text = RegistryServer::new(registry.clone()).with_output_mode(OutputMode::Text);
    assert!(matches!(server_text.output_mode(), OutputMode::Text));

    // Dual mode
    let server_dual = RegistryServer::new(registry).with_output_mode(OutputMode::Dual);
    assert!(matches!(server_dual.output_mode(), OutputMode::Dual));
}

#[test]
fn test_registry_has_tools() {
    let registry = create_test_registry();

    assert_eq!(registry.len(), 2);
    assert!(registry.contains("greet"));
    assert!(registry.contains("calculate"));
}

#[tokio::test]
async fn test_registry_dispatch_greet() {
    let registry = create_test_registry();
    let ctx = ToolContext::default();

    let args = serde_json::json!({
        "name": "World",
        "excited": true
    });

    let result = registry.dispatch_json("greet", args, &ctx).await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert_eq!(output["greeting"], "Hello, World!");
}

#[tokio::test]
async fn test_registry_dispatch_calculate() {
    let registry = create_test_registry();
    let ctx = ToolContext::default();

    let args = serde_json::json!({
        "a": 10,
        "b": 5,
        "operation": "mul"
    });

    let result = registry.dispatch_json("calculate", args, &ctx).await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert_eq!(output["result"], 50);
}

#[tokio::test]
async fn test_registry_dispatch_unknown_tool() {
    let registry = create_test_registry();
    let ctx = ToolContext::default();

    let args = serde_json::json!({});
    let result = registry.dispatch_json("nonexistent", args, &ctx).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown tool"));
}

#[tokio::test]
async fn test_registry_dispatch_invalid_args() {
    let registry = create_test_registry();
    let ctx = ToolContext::default();

    // Missing required 'name' field
    let args = serde_json::json!({
        "excited": true
    });

    let result = registry.dispatch_json("greet", args, &ctx).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_registry_dispatch_error_in_tool() {
    let registry = create_test_registry();
    let ctx = ToolContext::default();

    // Invalid operation
    let args = serde_json::json!({
        "a": 1,
        "b": 2,
        "operation": "invalid"
    });

    let result = registry.dispatch_json("calculate", args, &ctx).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unknown operation")
    );
}

#[test]
fn test_registry_subset() {
    let registry = create_test_registry();

    // Create subset with only greet
    let subset = registry.subset(["greet"]);

    assert_eq!(subset.len(), 1);
    assert!(subset.contains("greet"));
    assert!(!subset.contains("calculate"));
}

#[tokio::test]
async fn test_registry_subset_dispatch() {
    let registry = create_test_registry();
    let subset = registry.subset(["greet"]);
    let ctx = ToolContext::default();

    // Greet should work
    let args = serde_json::json!({"name": "Test"});
    let result = subset.dispatch_json("greet", args, &ctx).await;
    assert!(result.is_ok());

    // Calculate should fail (not in subset)
    let args = serde_json::json!({"a": 1, "b": 2, "operation": "add"});
    let result = subset.dispatch_json("calculate", args, &ctx).await;
    assert!(result.is_err());
}

#[test]
fn test_empty_registry() {
    let registry = Arc::new(ToolRegistry::builder().finish());
    let server = RegistryServer::new(registry.clone());

    assert!(registry.is_empty());
    assert_eq!(server.name(), "agentic-tools");
}

#[test]
fn test_tool_schemas() {
    let registry = create_test_registry();

    // Get greet tool
    let greet = registry.get("greet").unwrap();
    let schema = greet.input_schema();

    // Schema should have properties
    let schema_json = serde_json::to_value(&schema).unwrap();
    assert!(schema_json["properties"]["name"].is_object());
}

#[test]
fn test_tool_descriptions() {
    let registry = create_test_registry();

    let greet = registry.get("greet").unwrap();
    assert_eq!(greet.description(), "Greet someone by name");

    let calc = registry.get("calculate").unwrap();
    assert_eq!(calc.description(), "Perform arithmetic calculation");
}

// =============================================================================
// Output Schema Tests
// =============================================================================

#[test]
fn test_tool_output_schema() {
    let registry = create_test_registry();

    let greet = registry.get("greet").unwrap();
    let output_schema = greet.output_schema();

    // Should have output schema
    assert!(output_schema.is_some());

    let schema_json = serde_json::to_value(output_schema.unwrap()).unwrap();
    assert!(schema_json["properties"]["greeting"].is_object());
}

// =============================================================================
// Formatted Output (Dual Mode) Tests
// =============================================================================

#[tokio::test]
async fn test_dispatch_json_formatted_returns_text_and_data() {
    let registry = create_test_registry();
    let ctx = ToolContext::default();
    let text_opts = TextOptions::default();

    let args = serde_json::json!({
        "name": "World",
        "excited": true
    });

    let result = registry
        .dispatch_json_formatted("greet", args, &ctx, &text_opts)
        .await;

    assert!(result.is_ok());
    let formatted = result.unwrap();

    // Data should be JSON object
    assert_eq!(formatted.data["greeting"], "Hello, World!");

    // Text should be present (fallback to pretty JSON)
    assert!(formatted.text.is_some());
    let text = formatted.text.unwrap();
    assert!(text.contains("Hello, World!"));
}

#[tokio::test]
async fn test_dispatch_json_formatted_fallback_for_unknown_tool() {
    let registry = create_test_registry();
    let ctx = ToolContext::default();
    let text_opts = TextOptions::default();

    let args = serde_json::json!({});
    let result = registry
        .dispatch_json_formatted("nonexistent", args, &ctx, &text_opts)
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown tool"));
}
