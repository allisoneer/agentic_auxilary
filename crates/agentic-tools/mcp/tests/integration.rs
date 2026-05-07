//! Integration tests for MCP server functionality.
//!
//! These tests verify the `RegistryServer` logic without requiring
//! a full MCP transport layer.

use agentic_tools_core::TextFormat;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use agentic_tools_core::ToolRegistry;
use agentic_tools_core::fmt::TextOptions;
use agentic_tools_mcp::OutputMode;
use agentic_tools_mcp::RegistryServer;
use agentic_tools_mcp::ServerHandler;
use futures::future::BoxFuture;
use rmcp::ServiceExt;
use rmcp::model as m;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::Duration;
use tokio::time::timeout;

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

impl TextFormat for GreetOutput {}

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

#[derive(Clone)]
struct CancellationProbeTool {
    started: Arc<Notify>,
    observed_cancel: Arc<Notify>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct CancellationProbeInput {}

struct TestClient;

impl rmcp::ClientHandler for TestClient {}

impl Tool for CancellationProbeTool {
    type Input = CancellationProbeInput;
    type Output = String;
    const NAME: &'static str = "cancellation_probe";
    const DESCRIPTION: &'static str = "Waits for rmcp request cancellation";

    fn call(
        &self,
        _input: Self::Input,
        ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let started = Arc::clone(&self.started);
        let observed_cancel = Arc::clone(&self.observed_cancel);
        let ctx = ctx.clone();

        Box::pin(async move {
            started.notify_one();
            ctx.cancelled().await;

            if ctx.is_cancelled() {
                observed_cancel.notify_one();
            }

            Err(ToolError::cancelled(None))
        })
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct CalculateOutput {
    result: i32,
}

impl TextFormat for CalculateOutput {}

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

    // Text mode (default)
    let server_text = RegistryServer::new(Arc::clone(&registry)).with_output_mode(OutputMode::Text);
    assert!(matches!(server_text.output_mode(), OutputMode::Text));

    // Structured mode
    let server_structured = RegistryServer::new(registry).with_output_mode(OutputMode::Structured);
    assert!(matches!(
        server_structured.output_mode(),
        OutputMode::Structured
    ));
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
    let server = RegistryServer::new(Arc::clone(&registry));

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

#[tokio::test]
async fn test_rmcp_cancellation_flips_tool_context_mid_call() -> Result<(), String> {
    let started = Arc::new(Notify::new());
    let observed_cancel = Arc::new(Notify::new());
    let server = Arc::new(RegistryServer::new(Arc::new(
        ToolRegistry::builder()
            .register::<CancellationProbeTool, ()>(CancellationProbeTool {
                started: Arc::clone(&started),
                observed_cancel: Arc::clone(&observed_cancel),
            })
            .finish(),
    )));

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let (running, client) = tokio::try_join!(
        async {
            Arc::clone(&server)
                .serve(server_transport)
                .await
                .map_err(|err| err.to_string())
        },
        async {
            TestClient
                .serve(client_transport)
                .await
                .map_err(|err| err.to_string())
        },
    )?;

    let request_context =
        rmcp::service::RequestContext::new(m::NumberOrString::Number(1), running.peer().clone());
    let cancel = request_context.ct.clone();
    let server_for_call = Arc::clone(&server);
    let call_task = tokio::spawn(async move {
        server_for_call
            .call_tool(
                m::CallToolRequestParams::new("cancellation_probe"),
                request_context,
            )
            .await
            .map_err(|err| err.to_string())
    });

    timeout(Duration::from_secs(5), started.notified())
        .await
        .map_err(|_| "cancellation probe never started".to_string())?;

    cancel.cancel();

    timeout(Duration::from_secs(5), observed_cancel.notified())
        .await
        .map_err(|_| "tool context never observed cancellation".to_string())?;

    let tool_result = call_task.await.map_err(|err| err.to_string())??;

    assert_eq!(tool_result.is_error, Some(true));
    let content_json =
        serde_json::to_string(&tool_result.content).map_err(|err| err.to_string())?;
    assert!(content_json.contains("cancelled"));

    drop(client);
    drop(running);

    Ok(())
}
