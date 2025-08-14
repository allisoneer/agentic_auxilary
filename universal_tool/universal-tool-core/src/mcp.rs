//! MCP (Model Context Protocol) utilities for the Universal Tool Framework
//!
//! This module provides utilities for MCP server applications including
//! re-exports of rmcp and related dependencies.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

use crate::error::{ErrorCode, ToolError};
use schemars::JsonSchema;
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Re-export rmcp types so users don't need to depend on them
pub use rmcp;
pub use rmcp::*;

// Re-export commonly used MCP types for convenience
pub use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, CompleteRequestParam, CompleteResult, Content,
        ErrorData, GetPromptRequestParam, GetPromptResult, Implementation, InitializeRequestParam,
        InitializeResult, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParam, ReadResourceRequestParam, ReadResourceResult,
        ServerCapabilities, SetLevelRequestParam, SubscribeRequestParam, Tool,
        UnsubscribeRequestParam,
    },
    service::{RequestContext, RoleServer, Service, ServiceExt},
};

// Re-export Error as McpError for convenience
pub use rmcp::Error as McpError;

// Re-export stdio function for creating stdio transport
pub use rmcp::transport::stdio;

// Re-export serde_json for parameter handling
pub use serde_json::{Value as JsonValue, json};

/// MCP error data structure following JSON-RPC 2.0 spec
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct McpErrorData {
    pub code: i32,
    pub message: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Standard MCP error codes
pub mod error_codes {
    pub const RESOURCE_NOT_FOUND: i32 = -32002;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const PARSE_ERROR: i32 = -32700;
}

impl From<ToolError> for McpErrorData {
    fn from(err: ToolError) -> Self {
        let (code, message) = match err.code {
            ErrorCode::BadRequest => (error_codes::INVALID_REQUEST, err.message),
            ErrorCode::InvalidArgument => (error_codes::INVALID_PARAMS, err.message),
            ErrorCode::NotFound => (error_codes::RESOURCE_NOT_FOUND, err.message),
            ErrorCode::PermissionDenied => (error_codes::INVALID_REQUEST, err.message),
            ErrorCode::Internal => (error_codes::INTERNAL_ERROR, err.message),
            ErrorCode::Timeout => (error_codes::INTERNAL_ERROR, err.message),
            ErrorCode::Conflict => (error_codes::INVALID_REQUEST, err.message),
            ErrorCode::NetworkError => (error_codes::INTERNAL_ERROR, err.message),
            ErrorCode::ExternalServiceError => (error_codes::INTERNAL_ERROR, err.message),
            ErrorCode::ExecutionFailed => (error_codes::INTERNAL_ERROR, err.message),
            ErrorCode::SerializationError => (error_codes::PARSE_ERROR, err.message),
            ErrorCode::IoError => (error_codes::INTERNAL_ERROR, err.message),
        };

        McpErrorData {
            code,
            message: Cow::Owned(message),
            data: err.details.map(|d| Value::Object(d.into_iter().collect())),
        }
    }
}

/// Progress token for MCP progress reporting
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash, Eq)]
#[serde(transparent)]
pub struct ProgressToken(pub NumberOrString);

/// Progress notification parameters
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotification {
    pub progress_token: ProgressToken,
    pub progress: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Tool annotations for MCP metadata
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// Tool metadata for MCP discovery
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadata {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// Progress reporter trait for tools that support progress
#[async_trait::async_trait]
pub trait ProgressReporter: Send + Sync {
    /// Report progress for an operation
    async fn report(
        &self,
        progress: u32,
        total: Option<u32>,
        message: Option<String>,
    ) -> Result<(), ToolError>;
}

/// MCP-specific progress reporter that stores progress information
/// Users need to implement their own notification sending mechanism
pub struct McpProgressReporter {
    progress_token: ProgressToken,
    /// Channel for sending progress updates to the MCP server implementation
    sender: Option<tokio::sync::mpsc::Sender<ProgressNotification>>,
}

impl McpProgressReporter {
    /// Create a new progress reporter with a token
    pub fn new(progress_token: ProgressToken) -> Self {
        Self {
            progress_token,
            sender: None,
        }
    }

    /// Create a new progress reporter with a channel for sending updates
    pub fn with_sender(
        progress_token: ProgressToken,
        sender: tokio::sync::mpsc::Sender<ProgressNotification>,
    ) -> Self {
        Self {
            progress_token,
            sender: Some(sender),
        }
    }
}

#[async_trait::async_trait]
impl ProgressReporter for McpProgressReporter {
    async fn report(
        &self,
        current: u32,
        total: Option<u32>,
        message: Option<String>,
    ) -> Result<(), ToolError> {
        let notification = ProgressNotification {
            progress_token: self.progress_token.clone(),
            progress: current,
            total,
            message,
        };

        // If we have a sender, send the notification
        if let Some(sender) = &self.sender {
            sender.send(notification).await.map_err(|e| {
                ToolError::new(
                    ErrorCode::Internal,
                    format!("Failed to send progress notification: {e}"),
                )
            })?;
        }

        // Otherwise, just succeed silently (no-op progress reporter)
        Ok(())
    }
}

/// Re-export CancellationToken for convenience
pub use tokio_util::sync::CancellationToken;

/// Number or string type for MCP
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash, Eq)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(i64),
    String(String),
}

impl From<i64> for NumberOrString {
    fn from(n: i64) -> Self {
        NumberOrString::Number(n)
    }
}

impl From<String> for NumberOrString {
    fn from(s: String) -> Self {
        NumberOrString::String(s)
    }
}

impl From<&str> for NumberOrString {
    fn from(s: &str) -> Self {
        NumberOrString::String(s.to_string())
    }
}

thread_local! {
    static SCHEMA_CACHE: Arc<Mutex<HashMap<TypeId, Arc<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
}

/// Generate JSON Schema for a type implementing JsonSchema
/// Uses thread-local caching for efficient schema generation
pub fn generate_schema<T: JsonSchema + 'static>() -> Arc<Value> {
    let type_id = TypeId::of::<T>();

    SCHEMA_CACHE.with(|cache| {
        let mut cache_guard = cache.lock().unwrap();

        if let Some(schema) = cache_guard.get(&type_id) {
            return schema.clone();
        }

        // Generate schema using schemars with draft07
        let settings = schemars::r#gen::SchemaSettings::draft07();
        let generator = settings.into_generator();
        let schema = generator.into_root_schema_for::<T>();

        // Convert to Value
        let schema_value = serde_json::to_value(schema).unwrap_or(Value::Null);
        let arc_schema = Arc::new(schema_value);

        cache_guard.insert(type_id, arc_schema.clone());
        arc_schema
    })
}

/// Extract parameter schema for a tool
pub fn extract_parameter_schema<T: JsonSchema + 'static>() -> Value {
    let schema = generate_schema::<T>();
    // Extract just the schema portion (without $schema, title, etc)
    if let Some(obj) = schema.as_object() {
        // Clone the object and remove metadata fields
        let mut clean_schema = obj.clone();
        clean_schema.remove("$schema");
        clean_schema.remove("title");
        Value::Object(clean_schema)
    } else {
        (*schema).clone()
    }
}

/// Helper trait for converting tool results to MCP CallToolResult
pub trait IntoCallToolResult {
    /// Convert the result into a CallToolResult
    fn into_call_tool_result(self) -> CallToolResult;
}

impl<T: serde::Serialize> IntoCallToolResult for T {
    fn into_call_tool_result(self) -> CallToolResult {
        match serde_json::to_value(&self) {
            Ok(value) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
            )]),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to serialize result: {e}"
            ))]),
        }
    }
}

/// Helper function to convert UTF generated tool definitions to MCP Tool types
pub fn convert_tool_definitions(tool_jsons: Vec<JsonValue>) -> Vec<Tool> {
    use rmcp::model::ToolAnnotations;
    use std::sync::Arc;

    tool_jsons
        .into_iter()
        .filter_map(|json| {
            // Extract fields from the JSON
            let name = json.get("name")?.as_str()?.to_string();
            let description = json
                .get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string());

            // Convert input schema to the expected format
            let input_schema = if let Some(schema_value) = json.get("inputSchema") {
                schema_value
                    .as_object()
                    .map(|schema_obj| Arc::new(schema_obj.clone()))
            } else {
                None
            };

            // Extract annotations from the JSON
            // For now, we'll map our custom annotations to the available fields in rmcp
            let annotations = json
                .get("annotations")
                .map(|_hints| ToolAnnotations::default());

            Some(Tool {
                name: name.into(),
                description: description.map(|s| s.into()),
                input_schema: input_schema.unwrap_or_else(|| Arc::new(serde_json::Map::new())),
                annotations,
            })
        })
        .collect()
}

/// Helper macro to implement a basic MCP server
#[macro_export]
macro_rules! implement_mcp_server {
    ($struct_name:ident, $tools_field:ident) => {
        impl $crate::mcp::ServerHandler for $struct_name {
            fn initialize(
                &self,
                _params: $crate::mcp::InitializeRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::InitializeResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    Ok($crate::mcp::InitializeResult {
                        server_info: $crate::mcp::Implementation {
                            name: stringify!($struct_name).to_string().into(),
                            version: env!("CARGO_PKG_VERSION").to_string().into(),
                        },
                        capabilities: $crate::mcp::ServerCapabilities::builder()
                            .enable_tools()
                            .build(),
                        ..Default::default()
                    })
                }
            }

            fn list_tools(
                &self,
                _request: Option<$crate::mcp::PaginatedRequestParam>,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::ListToolsResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    let tools =
                        $crate::mcp::convert_tool_definitions(self.$tools_field.get_mcp_tools());
                    Ok($crate::mcp::ListToolsResult::with_all_items(tools))
                }
            }

            fn call_tool(
                &self,
                request: $crate::mcp::CallToolRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::CallToolResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    match self
                        .$tools_field
                        .handle_mcp_call(
                            &request.name,
                            $crate::mcp::JsonValue::Object(request.arguments.unwrap_or_default()),
                        )
                        .await
                    {
                        Ok(result) => Ok($crate::mcp::IntoCallToolResult::into_call_tool_result(
                            result,
                        )),
                        Err(e) => Ok($crate::mcp::CallToolResult::error(vec![
                            $crate::mcp::Content::text(format!("Error: {}", e)),
                        ])),
                    }
                }
            }

            // Default implementations for other required methods
            fn ping(
                &self,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<Output = Result<(), $crate::mcp::McpError>> + Send + '_
            {
                async move { Ok(()) }
            }

            fn complete(
                &self,
                _request: $crate::mcp::CompleteRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::CompleteResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    Err($crate::mcp::McpError::invalid_request(
                        "Method not implemented",
                        None,
                    ))
                }
            }

            fn set_level(
                &self,
                _request: $crate::mcp::SetLevelRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<Output = Result<(), $crate::mcp::McpError>> + Send + '_
            {
                async move { Ok(()) }
            }

            fn get_prompt(
                &self,
                _request: $crate::mcp::GetPromptRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::GetPromptResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    Err($crate::mcp::McpError::invalid_request(
                        "Method not implemented",
                        None,
                    ))
                }
            }

            fn list_prompts(
                &self,
                _request: Option<$crate::mcp::PaginatedRequestParam>,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::ListPromptsResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    Ok($crate::mcp::ListPromptsResult {
                        prompts: vec![],
                        next_cursor: None,
                    })
                }
            }

            fn list_resources(
                &self,
                _request: Option<$crate::mcp::PaginatedRequestParam>,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::ListResourcesResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    Ok($crate::mcp::ListResourcesResult {
                        resources: vec![],
                        next_cursor: None,
                    })
                }
            }

            fn list_resource_templates(
                &self,
                _request: Option<$crate::mcp::PaginatedRequestParam>,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::ListResourceTemplatesResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    Ok($crate::mcp::ListResourceTemplatesResult {
                        resource_templates: vec![],
                        next_cursor: None,
                    })
                }
            }

            fn read_resource(
                &self,
                _request: $crate::mcp::ReadResourceRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<
                Output = Result<$crate::mcp::ReadResourceResult, $crate::mcp::McpError>,
            > + Send
            + '_ {
                async move {
                    Err($crate::mcp::McpError::invalid_request(
                        "Method not implemented",
                        None,
                    ))
                }
            }

            fn subscribe(
                &self,
                _request: $crate::mcp::SubscribeRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<Output = Result<(), $crate::mcp::McpError>> + Send + '_
            {
                async move {
                    Err($crate::mcp::McpError::invalid_request(
                        "Method not implemented",
                        None,
                    ))
                }
            }

            fn unsubscribe(
                &self,
                _request: $crate::mcp::UnsubscribeRequestParam,
                _context: $crate::mcp::RequestContext<$crate::mcp::RoleServer>,
            ) -> impl ::std::future::Future<Output = Result<(), $crate::mcp::McpError>> + Send + '_
            {
                async move {
                    Err($crate::mcp::McpError::invalid_request(
                        "Method not implemented",
                        None,
                    ))
                }
            }
        }
    };
}
