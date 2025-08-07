// Re-export the error types
pub mod error;

// Re-export schemars types for use in generated code
pub use schemars::{self, JsonSchema};

// CLI utilities module (only available with cli feature)
// NOTE: We use #[cfg(feature = "cli")] to ensure CLI utilities are only available
// when the CLI feature is enabled. This keeps the API clean and follows idiomatic
// Rust practices. Testing confirms this approach works correctly with our examples.
#[cfg(feature = "cli")]
pub mod cli;

// REST utilities module (only available with rest feature)
#[cfg(feature = "rest")]
pub mod rest;

// MCP utilities module (only available with mcp feature)
#[cfg(feature = "mcp")]
pub mod mcp;

// Re-export the macros at the crate root for easy access
// IMPORTANT: Users should NOT depend on universal-tool-macros directly!
// These re-exports ensure that features propagate correctly. When you enable
// a feature on universal-tool-core, it automatically enables the corresponding
// feature on universal-tool-macros, ensuring only the requested code is generated.
pub use universal_tool_macros::{universal_tool, universal_tool_router};

/// The `prelude` module provides a single, convenient import for all essential
/// library items. This is just a convenience - users control their own application structure.
pub mod prelude {
    // Re-export the procedural macros from the other crate.
    // This allows the user to have a single `use` statement.
    pub use universal_tool_macros::{universal_tool, universal_tool_router};

    // Re-export the error types.
    pub use crate::error::{ToolError, ErrorCode};
    
    // Re-export JsonSchema for generated code
    pub use schemars::JsonSchema;

    // Re-export CLI utilities when the CLI feature is enabled
    #[cfg(feature = "cli")]
    pub use crate::cli::{OutputFormat, ProgressReporter, CliFormatter};
    
    // Re-export REST utilities when the REST feature is enabled
    #[cfg(feature = "rest")]
    pub use crate::rest::{
        // Core axum types for handlers
        axum,
        IntoResponse, Response, Json, Path, Query, State, StatusCode,
        // Tower utilities
        tower, tower_http,
    };
    
    // Re-export OpenAPI utilities when enabled
    #[cfg(feature = "openapi")]
    pub use crate::rest::{utoipa, utoipa_swagger_ui};
    
    // Re-export MCP utilities when the MCP feature is enabled
    #[cfg(feature = "mcp")]
    pub use crate::mcp::{
        // Core rmcp types and functions
        rmcp, ServerHandler, stdio,
        // MCP-specific types
        McpError, McpErrorData, McpProgressReporter,
        ProgressToken, ProgressNotification, NumberOrString,
        ToolAnnotations, ToolMetadata, CancellationToken,
        // Helper functions
        generate_schema, extract_parameter_schema, convert_tool_definitions,
        IntoCallToolResult,
        // Re-export commonly used values
        JsonValue, json,
        // Error codes
        error_codes,
    };
    
    // Re-export the MCP server macro (exported at crate root due to macro_export)
    #[cfg(feature = "mcp")]
    pub use crate::implement_mcp_server;
}
