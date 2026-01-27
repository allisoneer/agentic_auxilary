//! N-API bindings for agentic-tools.
//!
//! This crate provides JavaScript/TypeScript integration for the agentic-tools library family.
//! It enables calling Rust-based tools from Node.js, Bun, and other JavaScript runtimes.
//!
//! ## Generic Exports
//!
//! - `init(config)`: Initialize the tool registry with all available tools
//! - `listTools(provider)`: List available tools with schemas for a provider
//! - `callTool(name, args)`: Execute a tool with JSON arguments
//! - `setSchemaPatches(patches)`: Apply runtime schema transformations
//!
//! ## Typed Exports
//!
//! Per-tool typed wrappers for commonly used tools:
//! - `callLs(args)`: List files and directories
//! - `callGrep(args)`: Regex-based search
//! - `callGlob(args)`: Glob-based file matching
//! - `callAskAgent(args)`: Spawn Claude subagent
//! - `callJustSearch(args)`: Search justfile recipes
//! - `callJustExecute(args)`: Execute justfile recipe
//! - `callReasoningRequest(args)`: GPT-5 reasoning model request

#![deny(clippy::all)]

use agentic_tools_core::fmt::{TextOptions, fallback_text_from_json};
use agentic_tools_core::{FieldConstraint, SchemaEngine, ToolContext, ToolRegistry};
use agentic_tools_registry::{AgenticTools, AgenticToolsConfig};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::sync::Arc;

// =============================================================================
// Global State
// =============================================================================

/// Global registry instance, initialized once via `init()`.
static REGISTRY: OnceCell<Arc<ToolRegistry>> = OnceCell::new();

/// Global schema engine for runtime transforms.
static SCHEMA_ENGINE: OnceCell<RwLock<SchemaEngine>> = OnceCell::new();

// =============================================================================
// Initialization
// =============================================================================

/// Initialize the agentic-tools registry with all available tools.
///
/// Must be called before any other functions. The config JSON can specify
/// which tools to enable and other settings.
///
/// # Arguments
///
/// * `config_json` - JSON configuration string. Supports:
///   - `allowlist`: Array of tool names to enable (empty = all tools)
///   - `strict`: Boolean for strict schema mode (default: false)
///
/// # Example
///
/// ```typescript
/// import { init } from 'agentic-tools-napi';
/// init('{}'); // Initialize with all tools
/// init('{"allowlist": ["cli_ls", "cli_grep"]}'); // Only specific tools
/// ```
#[napi]
pub fn init(config_json: String) -> Result<()> {
    // Parse configuration
    let config: JsonValue = serde_json::from_str(&config_json)
        .map_err(|e| Error::from_reason(format!("Invalid config JSON: {}", e)))?;

    // Initialize schema engine
    let strict = config
        .get("strict")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let engine = SchemaEngine::new().with_strict(strict);
    SCHEMA_ENGINE
        .set(RwLock::new(engine))
        .map_err(|_| Error::from_reason("Schema engine already initialized"))?;

    // Parse allowlist from config
    let allowlist: Option<HashSet<String>> = config
        .get("allowlist")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });

    // Build registry via shared aggregator (includes all 19 tools)
    let registry = AgenticTools::new(AgenticToolsConfig {
        allowlist,
        extras: serde_json::json!({}),
    });

    REGISTRY
        .set(Arc::new(registry))
        .map_err(|_| Error::from_reason("Registry already initialized"))?;

    Ok(())
}

// =============================================================================
// Result Types
// =============================================================================

/// Result from a tool call, containing both human-readable text and JSON data.
///
/// - `data`: JSON string (for backward compatibility with consumers parsing strings)
/// - `text`: Human-oriented text representation
#[napi(object)]
pub struct ToolCallResult {
    /// JSON string containing the tool result data.
    pub data: String,
    /// Human-readable text representation of the result.
    pub text: String,
}

// =============================================================================
// Generic APIs
// =============================================================================

/// List available tools with their schemas for a specific provider.
///
/// Returns a JSON array of tool definitions formatted for the specified provider.
/// Schema patches are applied before rendering.
///
/// # Arguments
///
/// * `provider` - Provider format: "openai", "anthropic", or "mcp"
///
/// # Returns
///
/// JSON string containing an array of tool definitions.
#[napi]
pub fn list_tools(provider: String) -> Result<String> {
    let reg = REGISTRY
        .get()
        .ok_or_else(|| Error::from_reason("Registry not initialized. Call init() first."))?;

    let engine = SCHEMA_ENGINE.get().map(|e| e.read());
    let strict = engine.as_ref().map(|e| e.is_strict()).unwrap_or(false);
    let names = reg.list_names();

    let tools: Vec<JsonValue> = names
        .iter()
        .filter_map(|name| {
            let tool = reg.get(name)?;
            let base_schema = tool.input_schema();

            // Apply schema transforms if engine exists
            let schema = if let Some(ref eng) = engine {
                eng.transform(name, base_schema)
            } else {
                base_schema
            };

            Some(match provider.as_str() {
                "openai" => agentic_tools_core::providers::openai::render_function(
                    name,
                    tool.description(),
                    &schema,
                    strict,
                ),
                "anthropic" => agentic_tools_core::providers::anthropic::render_tool(
                    name,
                    tool.description(),
                    &schema,
                    strict,
                ),
                "mcp" => agentic_tools_core::providers::mcp::render_tool(
                    name,
                    tool.description(),
                    &schema,
                    tool.output_schema().as_ref(),
                ),
                _ => serde_json::json!({
                    "name": name,
                    "error": format!("Unknown provider: {}", provider)
                }),
            })
        })
        .collect();

    serde_json::to_string_pretty(&tools)
        .map_err(|e| Error::from_reason(format!("JSON serialization failed: {}", e)))
}

/// Execute a tool with JSON arguments.
///
/// # Arguments
///
/// * `name` - Name of the tool to call
/// * `args_json` - JSON string containing the tool arguments
///
/// # Returns
///
/// A `ToolCallResult` with both `text` (human-readable) and `data` (JSON string).
#[napi]
pub async fn call_tool(name: String, args_json: String) -> Result<ToolCallResult> {
    let reg = REGISTRY
        .get()
        .ok_or_else(|| Error::from_reason("Registry not initialized. Call init() first."))?;

    let args: JsonValue = serde_json::from_str(&args_json)
        .map_err(|e| Error::from_reason(format!("Invalid args JSON: {}", e)))?;

    let ctx = ToolContext::default();
    let text_opts = TextOptions::default();

    let result = reg
        .dispatch_json_formatted(&name, args, &ctx, &text_opts)
        .await
        .map_err(|e| Error::from_reason(format!("Tool execution failed: {}", e)))?;

    let data = serde_json::to_string(&result.data)
        .map_err(|e| Error::from_reason(format!("Result serialization failed: {}", e)))?;
    let text = result
        .text
        .unwrap_or_else(|| fallback_text_from_json(&result.data));

    Ok(ToolCallResult { data, text })
}

/// Apply schema patches for runtime customization.
///
/// Patches can modify tool schemas at runtime, enabling dynamic
/// enum values, field constraints, and other transformations.
///
/// # Arguments
///
/// * `patches_json` - JSON object where keys are tool names and values contain field patches.
///   Supported patch types:
///   - `enum`: Array of allowed values for a field
///   - `minimum`/`maximum`: Numeric range constraints
///   - `pattern`: Regex pattern for string validation
///
/// # Example
///
/// ```typescript
/// import { setSchemaPatches } from 'agentic-tools-napi';
/// setSchemaPatches(JSON.stringify({
///   "ask_agent": {
///     "properties": {
///       "agent_type": {
///         "enum": ["locator", "analyzer"]
///       }
///     }
///   }
/// }));
/// ```
#[napi]
pub fn set_schema_patches(patches_json: String) -> Result<()> {
    let engine_lock = SCHEMA_ENGINE
        .get()
        .ok_or_else(|| Error::from_reason("Schema engine not initialized. Call init() first."))?;

    let patches: JsonValue = serde_json::from_str(&patches_json)
        .map_err(|e| Error::from_reason(format!("Invalid patches JSON: {}", e)))?;

    let patches_obj = patches
        .as_object()
        .ok_or_else(|| Error::from_reason("Patches must be a JSON object"))?;

    let mut engine = engine_lock.write();

    for (tool_name, tool_patches) in patches_obj {
        if let Some(props) = tool_patches.get("properties").and_then(|p| p.as_object()) {
            for (field_name, field_patch) in props {
                // Handle enum constraint
                if let Some(enum_vals) = field_patch.get("enum").and_then(|e| e.as_array()) {
                    let values: Vec<JsonValue> = enum_vals.clone();
                    engine.constrain_field(
                        tool_name,
                        vec!["properties".to_string(), field_name.clone()],
                        FieldConstraint::Enum(values),
                    );
                }

                // Handle range constraints
                let minimum = field_patch.get("minimum").and_then(|v| v.as_f64());
                let maximum = field_patch.get("maximum").and_then(|v| v.as_f64());
                if minimum.is_some() || maximum.is_some() {
                    engine.constrain_field(
                        tool_name,
                        vec!["properties".to_string(), field_name.clone()],
                        FieldConstraint::Range { minimum, maximum },
                    );
                }

                // Handle pattern constraint
                if let Some(pattern) = field_patch.get("pattern").and_then(|p| p.as_str()) {
                    engine.constrain_field(
                        tool_name,
                        vec!["properties".to_string(), field_name.clone()],
                        FieldConstraint::Pattern(pattern.to_string()),
                    );
                }
            }
        }

        // Handle direct merge patch
        if tool_patches.get("properties").is_none() {
            engine.constrain_field(
                tool_name,
                vec![],
                FieldConstraint::MergePatch(tool_patches.clone()),
            );
        }
    }

    Ok(())
}

/// Check if the registry has been initialized.
#[napi]
pub fn is_initialized() -> bool {
    REGISTRY.get().is_some()
}

/// Get the number of registered tools.
#[napi]
pub fn tool_count() -> u32 {
    REGISTRY.get().map(|r| r.len() as u32).unwrap_or(0)
}

/// Get names of all registered tools.
#[napi]
pub fn get_tool_names() -> Result<Vec<String>> {
    let reg = REGISTRY
        .get()
        .ok_or_else(|| Error::from_reason("Registry not initialized. Call init() first."))?;
    Ok(reg.list_names())
}

// =============================================================================
// Typed Exports - coding_agent_tools
// =============================================================================

/// List files and directories (typed wrapper).
///
/// # Arguments
///
/// * `args_json` - JSON string with: path?, depth?, show?, ignore?, hidden?
///
/// # Returns
///
/// `ToolCallResult` with `text` and `data` (JSON string for LsOutput)
#[napi]
pub async fn call_ls(args_json: String) -> Result<ToolCallResult> {
    call_tool("cli_ls".to_string(), args_json).await
}

/// Ask a Claude subagent (typed wrapper).
///
/// # Arguments
///
/// * `args_json` - JSON string with: agent_type?, location?, query
///
/// # Returns
///
/// `ToolCallResult` with `text` and `data` (JSON string for AgentOutput)
#[napi]
pub async fn call_ask_agent(args_json: String) -> Result<ToolCallResult> {
    call_tool("ask_agent".to_string(), args_json).await
}

/// Regex-based search (typed wrapper).
///
/// # Arguments
///
/// * `args_json` - JSON string with: pattern, path?, mode?, globs?, ignore?, etc.
///
/// # Returns
///
/// `ToolCallResult` with `text` and `data` (JSON string for GrepOutput)
#[napi]
pub async fn call_grep(args_json: String) -> Result<ToolCallResult> {
    call_tool("cli_grep".to_string(), args_json).await
}

/// Glob-based file matching (typed wrapper).
///
/// # Arguments
///
/// * `args_json` - JSON string with: pattern, path?, ignore?, include_hidden?, sort?, head_limit?, offset?
///
/// # Returns
///
/// `ToolCallResult` with `text` and `data` (JSON string for GlobOutput)
#[napi]
pub async fn call_glob(args_json: String) -> Result<ToolCallResult> {
    call_tool("cli_glob".to_string(), args_json).await
}

/// Search justfile recipes (typed wrapper).
///
/// # Arguments
///
/// * `args_json` - JSON string with: query?, dir?
///
/// # Returns
///
/// `ToolCallResult` with `text` and `data` (JSON string for SearchOutput)
#[napi]
pub async fn call_just_search(args_json: String) -> Result<ToolCallResult> {
    call_tool("cli_just_search".to_string(), args_json).await
}

/// Execute a justfile recipe (typed wrapper).
///
/// # Arguments
///
/// * `args_json` - JSON string with: recipe, dir?, args?
///
/// # Returns
///
/// `ToolCallResult` with `text` and `data` (JSON string for ExecuteOutput)
#[napi]
pub async fn call_just_execute(args_json: String) -> Result<ToolCallResult> {
    call_tool("cli_just_execute".to_string(), args_json).await
}

// =============================================================================
// Typed Exports - gpt5_reasoner
// =============================================================================

/// Request assistance from the reasoning model (typed wrapper).
///
/// # Arguments
///
/// * `args_json` - JSON string with: prompt, files, prompt_type, directories?, output_filename?
///
/// # Returns
///
/// `ToolCallResult` with `text` and `data` (JSON string for reasoning result)
#[napi]
pub async fn call_reasoning_request(args_json: String) -> Result<ToolCallResult> {
    call_tool("ask_reasoning_model".to_string(), args_json).await
}
