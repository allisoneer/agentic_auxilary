//! MCP (Model Context Protocol) validation module.
//!
//! This module provides utilities for validating MCP server configurations
//! and tool whitelists before launching Claude sessions.

pub mod validate;

pub use validate::KNOWN_BUILTIN_TOOLS;
pub use validate::McpServerResult;
pub use validate::McpServerValidationError;
pub use validate::McpServerValidationSuccess;
pub use validate::McpValidationAggregateError;
pub use validate::McpValidationReport;
pub use validate::ToolWhitelistError;
pub use validate::ToolWhitelistReport;
pub use validate::TransportType;
pub use validate::ValidateOptions;
pub use validate::ensure_valid_mcp_config;
pub use validate::validate_mcp_config;
pub use validate::validate_tool_whitelist;
