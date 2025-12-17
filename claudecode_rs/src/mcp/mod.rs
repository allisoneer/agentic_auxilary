//! MCP (Model Context Protocol) validation module.
//!
//! This module provides utilities for validating MCP server configurations
//! and tool whitelists before launching Claude sessions.

pub mod validate;

pub use validate::{
    KNOWN_BUILTIN_TOOLS, McpServerResult, McpServerValidationError, McpServerValidationSuccess,
    McpValidationAggregateError, McpValidationReport, ToolWhitelistError, ToolWhitelistReport,
    TransportType, ValidateOptions, ensure_valid_mcp_config, validate_mcp_config,
    validate_tool_whitelist,
};
