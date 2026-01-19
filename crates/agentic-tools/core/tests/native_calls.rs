//! Tests for native (zero-JSON) tool calls via ToolHandle.
//!
//! These tests verify that tools can be called without JSON serialization
//! when using the ToolHandle API for cross-crate composition.

use agentic_tools_core::{
    TextFormat, Tool, ToolCodec, ToolContext, ToolError, ToolHandle, ToolRegistry,
};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// =============================================================================
// Test Tool Definitions
// =============================================================================

/// A simple echo tool for testing native calls.
#[derive(Clone)]
struct EchoTool;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct EchoInput {
    message: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct EchoOutput {
    echoed: String,
}

impl TextFormat for EchoOutput {}

impl Tool for EchoTool {
    type Input = EchoInput;
    type Output = EchoOutput;
    const NAME: &'static str = "echo";
    const DESCRIPTION: &'static str = "Echoes the input message";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            Ok(EchoOutput {
                echoed: input.message,
            })
        })
    }
}

/// A tool that performs computation without needing serde on internal types.
#[derive(Clone)]
struct ComputeTool;

/// Native input type - no serde bounds!
#[derive(Debug)]
struct ComputeInput {
    values: Vec<i32>,
}

/// Native output type - now needs Serialize for TextFormat.
#[derive(Debug, Serialize)]
struct ComputeOutput {
    sum: i32,
    count: usize,
}

impl TextFormat for ComputeOutput {}

impl Tool for ComputeTool {
    type Input = ComputeInput;
    type Output = ComputeOutput;
    const NAME: &'static str = "compute";
    const DESCRIPTION: &'static str = "Computes sum and count of values";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            Ok(ComputeOutput {
                sum: input.values.iter().sum(),
                count: input.values.len(),
            })
        })
    }
}

/// Wire types for ComputeTool (serde-compatible)
#[derive(Debug, Deserialize, JsonSchema)]
struct ComputeWireIn {
    values: Vec<i32>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct ComputeWireOut {
    sum: i32,
    count: usize,
}

impl TextFormat for ComputeWireOut {}

/// Custom codec that converts between wire and native types
struct ComputeCodec;

impl ToolCodec<ComputeTool> for ComputeCodec {
    type WireIn = ComputeWireIn;
    type WireOut = ComputeWireOut;

    fn decode(wire: Self::WireIn) -> Result<ComputeInput, ToolError> {
        Ok(ComputeInput {
            values: wire.values,
        })
    }

    fn encode(native: ComputeOutput) -> Result<Self::WireOut, ToolError> {
        Ok(ComputeWireOut {
            sum: native.sum,
            count: native.count,
        })
    }
}

// =============================================================================
// Native Call Tests
// =============================================================================

#[tokio::test]
async fn test_native_call_via_handle() {
    let registry = ToolRegistry::builder()
        .register::<EchoTool, ()>(EchoTool)
        .finish();

    // Get type-safe handle
    let handle: ToolHandle<EchoTool> = registry.handle::<EchoTool>().unwrap();

    // Create tool instance
    let tool = EchoTool;
    let ctx = ToolContext::default();

    // Native call - no JSON serialization!
    let input = EchoInput {
        message: "Hello, native!".to_string(),
    };
    let output = handle.call(&tool, input, &ctx).await.unwrap();

    assert_eq!(output.echoed, "Hello, native!");
}

#[tokio::test]
async fn test_native_call_with_custom_codec() {
    let registry = ToolRegistry::builder()
        .register::<ComputeTool, ComputeCodec>(ComputeTool)
        .finish();

    // Verify registration
    assert!(registry.contains("compute"));

    // Get handle for native calls
    let handle: ToolHandle<ComputeTool> = registry.handle::<ComputeTool>().unwrap();

    let tool = ComputeTool;
    let ctx = ToolContext::default();

    // Native call with non-serde types!
    let input = ComputeInput {
        values: vec![1, 2, 3, 4, 5],
    };
    let output = handle.call(&tool, input, &ctx).await.unwrap();

    assert_eq!(output.sum, 15);
    assert_eq!(output.count, 5);
}

#[tokio::test]
async fn test_json_dispatch_with_custom_codec() {
    let registry = ToolRegistry::builder()
        .register::<ComputeTool, ComputeCodec>(ComputeTool)
        .finish();

    let ctx = ToolContext::default();

    // JSON dispatch uses the codec
    let args = serde_json::json!({
        "values": [10, 20, 30]
    });
    let result = registry.dispatch_json("compute", args, &ctx).await.unwrap();

    assert_eq!(result["sum"], 60);
    assert_eq!(result["count"], 3);
}

#[tokio::test]
async fn test_handle_for_unregistered_tool_fails() {
    let registry = ToolRegistry::builder()
        .register::<EchoTool, ()>(EchoTool)
        .finish();

    // Try to get handle for unregistered tool
    let result = registry.handle::<ComputeTool>();

    assert!(result.is_err());
    let err = result.err().unwrap();
    assert!(err.to_string().contains("not registered"));
}

#[tokio::test]
async fn test_multiple_tools_native_calls() {
    let registry = ToolRegistry::builder()
        .register::<EchoTool, ()>(EchoTool)
        .register::<ComputeTool, ComputeCodec>(ComputeTool)
        .finish();

    assert_eq!(registry.len(), 2);

    // Call both tools natively
    let echo_handle: ToolHandle<EchoTool> = registry.handle::<EchoTool>().unwrap();
    let compute_handle: ToolHandle<ComputeTool> = registry.handle::<ComputeTool>().unwrap();

    let ctx = ToolContext::default();

    let echo_out = echo_handle
        .call(
            &EchoTool,
            EchoInput {
                message: "test".into(),
            },
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(echo_out.echoed, "test");

    let compute_out = compute_handle
        .call(&ComputeTool, ComputeInput { values: vec![1, 2] }, &ctx)
        .await
        .unwrap();
    assert_eq!(compute_out.sum, 3);
}

// =============================================================================
// Subset and Filtering Tests
// =============================================================================

#[tokio::test]
async fn test_subset_preserves_handles() {
    let registry = ToolRegistry::builder()
        .register::<EchoTool, ()>(EchoTool)
        .register::<ComputeTool, ComputeCodec>(ComputeTool)
        .finish();

    // Create subset with only echo
    let subset = registry.subset(["echo"]);

    assert_eq!(subset.len(), 1);
    assert!(subset.contains("echo"));
    assert!(!subset.contains("compute"));

    // Handle still works for included tool
    let handle: ToolHandle<EchoTool> = subset.handle::<EchoTool>().unwrap();
    let ctx = ToolContext::default();
    let out = handle
        .call(
            &EchoTool,
            EchoInput {
                message: "subset".into(),
            },
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(out.echoed, "subset");
}

#[tokio::test]
async fn test_subset_excludes_handles() {
    let registry = ToolRegistry::builder()
        .register::<EchoTool, ()>(EchoTool)
        .register::<ComputeTool, ComputeCodec>(ComputeTool)
        .finish();

    // Create subset without compute
    let subset = registry.subset(["echo"]);

    // Handle for excluded tool should fail
    let result = subset.handle::<ComputeTool>();
    assert!(result.is_err());
}

// =============================================================================
// Error Handling Tests
// =============================================================================

/// A tool that can fail
#[derive(Clone)]
struct FailingTool;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct FailingInput {
    should_fail: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct FailingOutput {
    success: bool,
}

impl TextFormat for FailingOutput {}

impl Tool for FailingTool {
    type Input = FailingInput;
    type Output = FailingOutput;
    const NAME: &'static str = "failing";
    const DESCRIPTION: &'static str = "A tool that can fail";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            if input.should_fail {
                Err(ToolError::internal("Intentional failure"))
            } else {
                Ok(FailingOutput { success: true })
            }
        })
    }
}

#[tokio::test]
async fn test_native_call_error_propagation() {
    let registry = ToolRegistry::builder()
        .register::<FailingTool, ()>(FailingTool)
        .finish();

    let handle: ToolHandle<FailingTool> = registry.handle::<FailingTool>().unwrap();
    let ctx = ToolContext::default();

    // Successful call
    let success = handle
        .call(&FailingTool, FailingInput { should_fail: false }, &ctx)
        .await;
    assert!(success.is_ok());

    // Failing call
    let failure = handle
        .call(&FailingTool, FailingInput { should_fail: true }, &ctx)
        .await;
    assert!(failure.is_err());
    assert!(
        failure
            .unwrap_err()
            .to_string()
            .contains("Intentional failure")
    );
}

#[tokio::test]
async fn test_json_dispatch_error_propagation() {
    let registry = ToolRegistry::builder()
        .register::<FailingTool, ()>(FailingTool)
        .finish();

    let ctx = ToolContext::default();

    // Failing JSON dispatch
    let args = serde_json::json!({ "should_fail": true });
    let result = registry.dispatch_json("failing", args, &ctx).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Intentional failure")
    );
}
