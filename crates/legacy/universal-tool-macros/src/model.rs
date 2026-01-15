//! Internal model representation for the Universal Tool Framework macros.
//!
//! This module defines the clean, validated representation of tool definitions
//! that is used for code generation. It's independent of syn types to make
//! the code generation cleaner.

use proc_macro2::Ident;
use syn::Type;

/// Top-level definition for an entire impl block annotated with `#[universal_tool_router]`.
#[derive(Debug)]
#[allow(dead_code)]
pub struct RouterDef {
    /// The type the impl is for (e.g., `MyTools`)
    pub struct_type: syn::Path,
    /// The generics on the impl block, if any
    pub generics: Option<syn::Generics>,
    /// All tool methods found in the impl block
    pub tools: Vec<ToolDef>,
    /// Metadata from the router attribute
    pub metadata: RouterMetadata,
}

/// Definition for a single tool method.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ToolDef {
    /// The name of the method (e.g., `analyze_code`)
    pub method_name: Ident,
    /// The name to use for the tool in interfaces (defaults to method_name)
    pub tool_name: String,
    /// Method parameters (excluding &self)
    pub params: Vec<ParamDef>,
    /// The return type of the method
    pub return_type: Type,
    /// Metadata from the tool attribute
    pub metadata: ToolMetadata,
    /// Whether the method is async
    pub is_async: bool,
    /// Visibility of the method
    pub visibility: syn::Visibility,
}

/// Definition for a single parameter.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParamDef {
    /// Parameter name
    pub name: Ident,
    /// Parameter type
    pub ty: Type,
    /// Where this parameter comes from in different interfaces
    pub source: ParamSource,
    /// Whether this parameter is optional (e.g., Option<T>)
    pub is_optional: bool,
    /// Parameter metadata from attributes
    pub metadata: ParamMetadata,
}

/// Where a parameter comes from in different interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParamSource {
    /// Default - comes from request body in REST/MCP, command args in CLI
    #[default]
    Body,
    /// REST query parameters
    Query,
    /// REST path parameters
    Path,
    /// REST headers
    Header,
}

/// Metadata for the router attribute.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct RouterMetadata {
    /// OpenAPI tag for REST endpoints
    pub openapi_tag: Option<String>,
    /// Base path for REST endpoints
    pub base_path: Option<String>,
    /// CLI configuration
    pub cli_config: Option<RouterCliConfig>,
    /// MCP configuration (router-level)
    pub mcp_config: Option<RouterMcpConfig>,
    /// REST configuration (router-level)
    pub rest_config: Option<RouterRestConfig>,
}

/// Router-level CLI configuration.
#[derive(Debug, Default)]
pub struct RouterCliConfig {
    /// CLI command name
    pub name: Option<String>,
    /// CLI command description
    pub description: Option<String>,
    /// Global output formats available for all commands
    pub global_output_formats: Vec<String>,
    /// Whether to add standard global args (--dry-run, --yes, --quiet, --verbose)
    pub standard_global_args: bool,
}

/// Router-level MCP configuration.
#[derive(Debug, Default)]
pub struct RouterMcpConfig {
    /// MCP server name
    pub name: Option<String>,
    /// MCP server version
    pub version: Option<String>,
}

/// Router-level REST configuration.
#[derive(Debug, Default)]
pub struct RouterRestConfig {
    /// Base prefix for all REST endpoints
    pub prefix: Option<String>,
}

/// Metadata for individual tool methods.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct ToolMetadata {
    /// Tool description
    pub description: String,
    /// Short description for CLI help
    pub short_description: Option<String>,
    /// REST-specific configuration
    pub rest_config: Option<RestConfig>,
    /// MCP-specific configuration
    pub mcp_config: Option<McpConfig>,
    /// CLI-specific configuration
    pub cli_config: Option<CliConfig>,
}

/// REST API configuration for a tool.
#[derive(Debug)]
pub struct RestConfig {
    /// Custom path for this endpoint
    pub path: Option<String>,
    /// HTTP method
    pub method: HttpMethod,
}

/// HTTP methods supported by REST endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpMethod {
    Get,
    #[default]
    Post,
    Put,
    Delete,
    Patch,
}

/// MCP output mode for tool responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpOutputMode {
    Json,
    Text,
}

/// MCP-specific configuration.
#[derive(Debug, Default)]
pub struct McpConfig {
    /// MCP annotations for the tool
    pub annotations: McpAnnotations,
    /// Output mode for this tool (Text or Json)
    pub output_mode: Option<McpOutputMode>,
}

/// MCP tool annotations.
#[derive(Debug, Default)]
pub struct McpAnnotations {
    /// Hint that tool only reads data
    pub read_only_hint: Option<bool>,
    /// Hint that tool performs destructive operations
    pub destructive_hint: Option<bool>,
    /// Hint that tool is idempotent
    pub idempotent_hint: Option<bool>,
    /// Hint that tool accepts additional parameters beyond those specified
    pub open_world_hint: Option<bool>,
}

/// CLI-specific configuration.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct CliConfig {
    /// Command name override
    pub name: Option<String>,
    /// Alternative names for the command
    pub aliases: Vec<String>,
    /// Whether to hide this command from help
    pub hidden: bool,
    /// Supported output formats (json, yaml, table, text)
    pub output_formats: Vec<String>,
    /// Progress indicator style (bar, spinner, dots)
    pub progress_style: Option<String>,
    /// Examples to show in help text
    pub examples: Vec<CliExample>,
    /// Whether this command supports stdin input
    pub supports_stdin: bool,
    /// Whether this command supports stdout piping
    pub supports_stdout: bool,
    /// Confirmation prompt message (for destructive operations)
    pub confirm: Option<String>,
    /// Whether to enable interactive mode for parameter collection
    pub interactive: bool,
    /// Command path for nested subcommands (e.g., ["database", "migrate"])
    pub command_path: Vec<String>,
}

/// Example for CLI help text.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CliExample {
    /// The example command
    pub command: String,
    /// Description of what the example does
    pub description: String,
}

/// Parameter metadata from attributes.
#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct ParamMetadata {
    /// Parameter description
    pub description: Option<String>,
    /// Short name for CLI flags
    pub short: Option<char>,
    /// Long name override for CLI
    pub long: Option<String>,
    /// Environment variable to read from
    pub env: Option<String>,
    /// Default value (as a string to parse)
    pub default: Option<String>,
    /// Possible values for enum-like parameters
    pub possible_values: Vec<String>,
    /// Shell completion hint (file, dir, command:..., or static list)
    pub completions: Option<String>,
    /// Whether this parameter accepts multiple values (for Vec types)
    pub multiple: bool,
    /// Value delimiter for multiple values (default: comma)
    pub delimiter: Option<char>,
}

/// Validation result for tool definitions.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ValidationError {
    pub span: proc_macro2::Span,
    pub message: String,
    pub help: Option<String>,
}

impl RouterDef {
    /// Validate the router definition.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check for duplicate tool names
        let mut tool_names = std::collections::HashSet::new();
        for tool in &self.tools {
            if !tool_names.insert(&tool.tool_name) {
                errors.push(ValidationError {
                    span: tool.method_name.span(),
                    message: format!("Duplicate tool name: {}", tool.tool_name),
                    help: Some(
                        "Use the 'name' attribute to specify a different tool name".to_string(),
                    ),
                });
            }
        }

        // Validate each tool
        for tool in &self.tools {
            if let Err(tool_errors) = tool.validate() {
                errors.extend(tool_errors);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl ToolDef {
    /// Extract path parameter names from a REST path like "/projects/:project_id/tasks/:task_id"
    fn extract_path_params(&self, path: &str) -> Vec<String> {
        let mut params = Vec::new();
        for segment in path.split('/') {
            if let Some(stripped) = segment.strip_prefix(':') {
                params.push(stripped.to_string());
            }
        }
        params
    }

    /// Validate the tool definition.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Validate return type is Result<T, ToolError>
        if !self.is_valid_return_type() {
            errors.push(ValidationError {
                span: proc_macro2::Span::call_site(),
                message: "Tool methods must return Result<T, ToolError>".to_string(),
                help: Some("Change your return type to Result<YourType, ToolError>".to_string()),
            });
        }

        // Validate parameter names don't conflict
        let mut param_names = std::collections::HashSet::new();
        for param in &self.params {
            if !param_names.insert(&param.name) {
                errors.push(ValidationError {
                    span: param.name.span(),
                    message: format!("Duplicate parameter name: {}", param.name),
                    help: None,
                });
            }
        }

        // Validate REST configuration
        if let Some(rest_config) = &self.metadata.rest_config {
            // Extract path parameters from the REST path
            let path_params = if let Some(path) = &rest_config.path {
                self.extract_path_params(path)
            } else {
                Vec::new()
            };

            // GET requests shouldn't have body parameters
            if rest_config.method == HttpMethod::Get {
                for param in &self.params {
                    // Skip validation if this is a path parameter
                    if path_params.contains(&param.name.to_string()) {
                        continue;
                    }

                    if param.source == ParamSource::Body {
                        errors.push(ValidationError {
                            span: param.name.span(),
                            message: "GET requests cannot have body parameters".to_string(),
                            help: Some("Use #[universal_tool_param(source = \"query\")] or change the HTTP method".to_string()),
                        });
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Check if the return type is Result<T, ToolError>.
    fn is_valid_return_type(&self) -> bool {
        // This is a simplified check - the actual implementation would
        // properly parse the type and verify it's Result<T, ToolError>
        // For now, we'll implement this in the parser
        true
    }
}
