//! Integration tests for the MCP example
//!
//! These tests verify that the UTF-generated MCP methods work correctly
//! without requiring a full MCP server setup.

use serde_json::json;
use universal_tool_core::prelude::*;

/// Test struct matching the example
#[derive(Clone)]
struct TestTextTools;

#[universal_tool_router]
impl TestTextTools {
    #[universal_tool(description = "Count words in text")]
    async fn count_words(&self, text: String) -> Result<usize, ToolError> {
        Ok(text.split_whitespace().count())
    }

    #[universal_tool(description = "Reverse text")]
    async fn reverse(&self, text: String) -> Result<String, ToolError> {
        Ok(text.chars().rev().collect())
    }
}

#[tokio::test]
async fn test_get_mcp_tools() {
    let tools = TestTextTools;

    // Get the tool definitions
    let tool_defs = tools.get_mcp_tools();

    // Verify we have the expected number of tools
    assert_eq!(tool_defs.len(), 2);

    // Check first tool
    let count_tool = &tool_defs[0];
    assert_eq!(count_tool["name"], "count_words");
    assert_eq!(count_tool["description"], "Count words in text");

    // Verify input schema
    let schema = &count_tool["inputSchema"];
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["text"].is_object());
    assert_eq!(schema["properties"]["text"]["type"], "string");
    assert_eq!(schema["required"], json!(["text"]));

    // Check second tool
    let reverse_tool = &tool_defs[1];
    assert_eq!(reverse_tool["name"], "reverse");
    assert_eq!(reverse_tool["description"], "Reverse text");
}

#[tokio::test]
async fn test_handle_mcp_call_count_words() {
    let tools = TestTextTools;

    // Call count_words tool
    let params = json!({
        "text": "hello world test"
    });

    let result = tools.handle_mcp_call("count_words", params).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value, json!(3));
}

#[tokio::test]
async fn test_handle_mcp_call_reverse() {
    let tools = TestTextTools;

    // Call reverse tool
    let params = json!({
        "text": "hello"
    });

    let result = tools.handle_mcp_call("reverse", params).await;
    assert!(result.is_ok());

    let value = result.unwrap();
    assert_eq!(value, json!("olleh"));
}

#[tokio::test]
async fn test_handle_mcp_call_unknown_method() {
    let tools = TestTextTools;

    let params = json!({});
    let result = tools.handle_mcp_call("unknown_method", params).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unknown method"));
}

#[tokio::test]
async fn test_handle_mcp_call_invalid_params() {
    let tools = TestTextTools;

    // Missing required parameter
    let params = json!({});
    let result = tools.handle_mcp_call("count_words", params).await;

    assert!(result.is_err());
    // The error should be about missing parameter
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Missing required parameter"));
}
