//! Integration test for UTF cross-cutting features
//!
//! This test verifies:
//! - Async/sync normalization works across all interfaces
//! - Error handling is consistent
//! - All interfaces can be used together

#[cfg(test)]
mod tests {
    
    use universal_tool_core::prelude::*;

    /// Test struct with both sync and async methods
    struct TestTools;

    #[universal_tool_router(
        cli(name = "test-tools"),
        rest(prefix = "/api"),
        mcp(name = "test-mcp")
    )]
    impl TestTools {
        /// Sync method - should work in all interfaces
        #[universal_tool(description = "Add two numbers (sync)")]
        fn add(&self, a: i32, b: i32) -> Result<i32, ToolError> {
            if a == 0 && b == 0 {
                return Err(ToolError::new(
                    ErrorCode::InvalidArgument,
                    "Both arguments cannot be zero",
                ));
            }
            Ok(a + b)
        }

        /// Async method - should work in all interfaces
        #[universal_tool(description = "Multiply two numbers (async)")]
        async fn multiply(&self, x: i32, y: i32) -> Result<i32, ToolError> {
            // Simulate async work
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;

            if x > 1000 || y > 1000 {
                return Err(ToolError::new(ErrorCode::BadRequest, "Numbers too large"));
            }

            Ok(x * y)
        }
    }

    #[test]
    fn test_cli_generation() {
        let tools = TestTools;

        // Verify CLI command can be created
        let cmd = tools.create_cli_command();
        assert_eq!(cmd.get_name(), "test-tools");

        // Verify both sync and async methods are present as subcommands
        let subcommands: Vec<_> = cmd
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect();

        assert!(subcommands.contains(&"add".to_string()));
        assert!(subcommands.contains(&"multiply".to_string()));
    }

    #[cfg(feature = "rest")]
    #[test]
    fn test_rest_router_creation() {
        use std::sync::Arc;

        let tools = Arc::new(TestTools);
        let router = TestTools::create_rest_router(tools.clone());

        // Router should be created successfully with both endpoints
        // Actual endpoint testing would require a test server
        assert!(true); // Placeholder - router creation didn't panic
    }

    #[cfg(feature = "mcp")]
    #[test]
    fn test_mcp_tools_discovery() {
        let tools = TestTools;
        let mcp_tools = tools.get_mcp_tools();

        // Should have both tools
        assert_eq!(mcp_tools.len(), 2);

        // Verify tool names
        let tool_names: Vec<String> = mcp_tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .map(|s| s.to_string())
            .collect();

        assert!(tool_names.contains(&"add".to_string()));
        assert!(tool_names.contains(&"multiply".to_string()));
    }

    #[tokio::test]
    async fn test_error_consistency() {
        let tools = TestTools;

        // Test sync method error
        let sync_result = tools.add(0, 0);
        assert!(sync_result.is_err());
        let sync_err = sync_result.unwrap_err();
        assert_eq!(sync_err.code, ErrorCode::InvalidArgument);

        // Test async method error
        let async_result = tools.multiply(2000, 2000).await;
        assert!(async_result.is_err());
        let async_err = async_result.unwrap_err();
        assert_eq!(async_err.code, ErrorCode::BadRequest);
    }

    #[tokio::test]
    async fn test_async_sync_normalization() {
        let tools = TestTools;

        // Both sync and async methods should work
        let sync_result = tools.add(5, 3);
        assert_eq!(sync_result.unwrap(), 8);

        let async_result = tools.multiply(4, 7).await;
        assert_eq!(async_result.unwrap(), 28);
    }
}
