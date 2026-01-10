//! Provider-specific schema renderers.
//!
//! Each provider has slightly different requirements for tool schemas:
//! - OpenAI: `{"type": "function", "function": {...}}`
//! - Anthropic: Direct object with `input_schema` field
//! - MCP: Direct object with `inputSchema`/`outputSchema`

pub mod anthropic;
pub mod mcp;
pub mod openai;
