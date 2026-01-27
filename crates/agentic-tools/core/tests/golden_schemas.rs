//! Golden tests for provider schema rendering.
//!
//! These tests verify that schema rendering produces consistent output
//! across all supported providers (OpenAI, Anthropic, MCP).

use agentic_tools_core::providers::{anthropic, mcp, openai};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

// =============================================================================
// Test Schema Definitions
// =============================================================================

/// Simple tool input for testing.
#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct SimpleInput {
    /// A required string message
    message: String,
    /// An optional count
    #[serde(default)]
    count: Option<i32>,
}

/// Complex tool input with nested types.
#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct ComplexInput {
    /// The query to execute
    query: String,
    /// Configuration options
    options: Options,
    /// List of tags
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct Options {
    /// Enable verbose output
    verbose: bool,
    /// Maximum results to return
    limit: Option<u32>,
}

/// Output type for testing
#[derive(Debug, Serialize, JsonSchema)]
#[allow(dead_code)]
struct SimpleOutput {
    result: String,
    success: bool,
}

// =============================================================================
// OpenAI Provider Golden Tests
// =============================================================================

#[test]
fn golden_openai_simple_schema() {
    let schema = schema_for!(SimpleInput);
    let rendered =
        openai::render_function("simple_tool", "A simple tool for testing", &schema, true);

    // Verify structure
    assert_eq!(rendered["type"], "function");
    assert_eq!(rendered["function"]["name"], "simple_tool");
    assert_eq!(
        rendered["function"]["description"],
        "A simple tool for testing"
    );
    assert_eq!(rendered["function"]["strict"], true);

    // Verify parameters schema exists
    let params = &rendered["function"]["parameters"];
    assert!(params.is_object());
    assert!(params["properties"]["message"].is_object());
}

#[test]
fn golden_openai_complex_schema() {
    let schema = schema_for!(ComplexInput);
    let rendered = openai::render_function("complex_tool", "A complex tool", &schema, true);

    let params = &rendered["function"]["parameters"];

    // Verify nested structure
    assert!(params["properties"]["query"].is_object());
    assert!(params["properties"]["options"].is_object());
    assert!(params["properties"]["tags"].is_object());

    // Verify definitions for nested types exist
    // (schemars generates $defs for complex types)
    let defs = &params["$defs"];
    if defs.is_object() {
        // Options should be referenced
        assert!(
            defs["Options"].is_object()
                || params["properties"]["options"]["properties"].is_object()
        );
    }
}

#[test]
fn golden_openai_schema_determinism() {
    let schema = schema_for!(SimpleInput);

    // Render twice and compare
    let rendered1 = openai::render_function("test", "desc", &schema, true);
    let rendered2 = openai::render_function("test", "desc", &schema, true);

    assert_eq!(
        rendered1, rendered2,
        "Schema rendering should be deterministic"
    );
}

// =============================================================================
// Anthropic Provider Golden Tests
// =============================================================================

#[test]
fn golden_anthropic_simple_schema() {
    let schema = schema_for!(SimpleInput);
    let rendered =
        anthropic::render_tool("simple_tool", "A simple tool for testing", &schema, true);

    // Verify Anthropic format (direct object, not wrapped in "function")
    assert_eq!(rendered["name"], "simple_tool");
    assert_eq!(rendered["description"], "A simple tool for testing");
    assert_eq!(rendered["strict"], true);

    // Anthropic uses input_schema, not parameters
    assert!(rendered["input_schema"].is_object());
}

#[test]
fn golden_anthropic_complex_schema() {
    let schema = schema_for!(ComplexInput);
    let rendered = anthropic::render_tool("complex_tool", "A complex tool", &schema, true);

    let input_schema = &rendered["input_schema"];

    // Verify properties exist
    assert!(input_schema["properties"]["query"].is_object());
    assert!(input_schema["properties"]["options"].is_object());
}

#[test]
fn golden_anthropic_non_strict() {
    let schema = schema_for!(SimpleInput);
    let rendered = anthropic::render_tool("tool", "desc", &schema, false);

    assert_eq!(rendered["strict"], false);
}

// =============================================================================
// MCP Provider Golden Tests
// =============================================================================

#[test]
fn golden_mcp_simple_schema() {
    let schema = schema_for!(SimpleInput);
    let rendered = mcp::render_tool("simple_tool", "A simple tool for testing", &schema, None);

    // Verify MCP format
    assert_eq!(rendered["name"], "simple_tool");
    assert_eq!(rendered["description"], "A simple tool for testing");

    // MCP uses inputSchema
    assert!(rendered["inputSchema"].is_object());

    // No outputSchema when None passed
    assert!(rendered.get("outputSchema").is_none());
}

#[test]
fn golden_mcp_with_output_schema() {
    let input_schema = schema_for!(SimpleInput);
    let output_schema = schema_for!(SimpleOutput);
    let rendered = mcp::render_tool("tool", "desc", &input_schema, Some(&output_schema));

    // Both schemas present
    assert!(rendered["inputSchema"].is_object());
    assert!(rendered["outputSchema"].is_object());

    // Output schema has expected structure
    let output = &rendered["outputSchema"];
    assert!(output["properties"]["result"].is_object());
    assert!(output["properties"]["success"].is_object());
}

#[test]
fn golden_mcp_complex_schema() {
    let schema = schema_for!(ComplexInput);
    let rendered = mcp::render_tool("complex_tool", "A complex tool", &schema, None);

    let input_schema = &rendered["inputSchema"];

    // Verify nested structure preserved
    assert!(input_schema["properties"]["query"].is_object());
    assert!(input_schema["properties"]["options"].is_object());
}

// =============================================================================
// Cross-Provider Consistency Tests
// =============================================================================

#[test]
fn golden_cross_provider_consistency() {
    let schema = schema_for!(SimpleInput);

    let openai = openai::render_function("test", "Test tool", &schema, true);
    let anthropic = anthropic::render_tool("test", "Test tool", &schema, true);
    let mcp = mcp::render_tool("test", "Test tool", &schema, None);

    // All should have the same tool name and description
    assert_eq!(openai["function"]["name"], "test");
    assert_eq!(anthropic["name"], "test");
    assert_eq!(mcp["name"], "test");

    // Schema content should be equivalent (even if keys differ)
    let openai_props = &openai["function"]["parameters"]["properties"];
    let anthropic_props = &anthropic["input_schema"]["properties"];
    let mcp_props = &mcp["inputSchema"]["properties"];

    // All should have "message" property
    assert!(openai_props["message"].is_object());
    assert!(anthropic_props["message"].is_object());
    assert!(mcp_props["message"].is_object());
}

// =============================================================================
// Schema Transform Integration Tests
// =============================================================================

#[test]
fn golden_schema_with_transforms() {
    use agentic_tools_core::{FieldConstraint, SchemaEngine};

    let schema = schema_for!(SimpleInput);
    let mut engine = SchemaEngine::new();

    // Add enum constraint
    engine.constrain_field(
        "test",
        vec!["properties".to_string(), "message".to_string()],
        FieldConstraint::Enum(vec![serde_json::json!("hello"), serde_json::json!("world")]),
    );

    let transformed = engine.transform("test", schema);
    let rendered = openai::render_function("test", "desc", &transformed, true);

    // Verify enum was applied
    let message_schema = &rendered["function"]["parameters"]["properties"]["message"];
    assert!(
        message_schema.get("enum").is_some(),
        "Enum constraint should be applied to schema"
    );
}

#[test]
fn golden_schema_with_range_constraint() {
    use agentic_tools_core::{FieldConstraint, SchemaEngine};

    let schema = schema_for!(SimpleInput);
    let mut engine = SchemaEngine::new();

    // Add range constraint to count
    engine.constrain_field(
        "test",
        vec!["properties".to_string(), "count".to_string()],
        FieldConstraint::Range {
            minimum: Some(serde_json::json!(0)),
            maximum: Some(serde_json::json!(100)),
        },
    );

    let transformed = engine.transform("test", schema);
    let transformed_json = serde_json::to_value(&transformed).unwrap();

    let count_schema = &transformed_json["properties"]["count"];

    // Verify range was applied (compare as f64 since schemars may use floats)
    let min = count_schema.get("minimum").and_then(|v| v.as_f64());
    let max = count_schema.get("maximum").and_then(|v| v.as_f64());

    assert_eq!(min, Some(0.0));
    assert_eq!(max, Some(100.0));
}
