//! MCP (Model Context Protocol) code generation for Universal Tool Framework
//!
//! This module generates `handle_mcp_call()` and `get_mcp_tools()` methods
//! that enable tools to integrate with MCP servers.

use crate::codegen::shared::is_optional_type;
use crate::codegen::validation;
use crate::model::{ParamDef, RouterDef, ToolDef};
use proc_macro2::TokenStream;
use quote::quote;

/// Generates all MCP-related methods for a router
pub fn generate_mcp_methods(router: &RouterDef) -> TokenStream {
    let struct_type = &router.struct_type;
    let dispatch_method = generate_mcp_dispatch_method(router);
    let dispatch_method_mcp = generate_mcp_dispatch_method_mcp(router);
    let tools_method = generate_mcp_tools_method(router);
    let server_info_method = generate_mcp_server_info_method(router);

    quote! {
        impl #struct_type {
            #dispatch_method

            #dispatch_method_mcp

            #tools_method

            #server_info_method
        }
    }
}

/// Generates the handle_mcp_call() method for dispatching MCP tool calls
fn generate_mcp_dispatch_method(router: &RouterDef) -> TokenStream {
    let match_arms: Vec<_> = router.tools.iter().map(generate_tool_match_arm).collect();

    quote! {
        /// Handles MCP tool calls by dispatching to the appropriate tool method
        /// Users integrate this into their MCP server implementation
        pub async fn handle_mcp_call(
            &self,
            method: &str,
            params: ::serde_json::Value
        ) -> ::std::result::Result<::serde_json::Value, ::universal_tool_core::error::ToolError> {
            match method {
                #(#match_arms)*
                _ => {
                    ::std::result::Result::Err(
                        ::universal_tool_core::error::ToolError::new(
                            ::universal_tool_core::error::ErrorCode::NotFound,
                            ::std::format!("Unknown method: {}", method)
                        )
                    )
                }
            }
        }
    }
}

/// Generates the handle_mcp_call_mcp() method that returns McpOutput (Text or Json)
fn generate_mcp_dispatch_method_mcp(router: &RouterDef) -> TokenStream {
    let match_arms: Vec<_> = router
        .tools
        .iter()
        .map(generate_tool_match_arm_mcp)
        .collect();

    quote! {
        /// Handles MCP tool calls and returns McpOutput (Text or Json), honoring per-tool output mode.
        pub async fn handle_mcp_call_mcp(
            &self,
            method: &str,
            params: ::serde_json::Value
        ) -> ::std::result::Result<::universal_tool_core::mcp::McpOutput, ::universal_tool_core::error::ToolError> {
            match method {
                #(#match_arms)*
                _ => {
                    ::std::result::Result::Err(
                        ::universal_tool_core::error::ToolError::new(
                            ::universal_tool_core::error::ErrorCode::NotFound,
                            ::std::format!("Unknown method: {}", method)
                        )
                    )
                }
            }
        }
    }
}

/// Generates a match arm for a single tool
fn generate_tool_match_arm(tool: &ToolDef) -> TokenStream {
    let tool_name = &tool.tool_name;

    // Generate per-field extraction bindings for MCP
    let param_extractions = crate::codegen::validation::generate_params_extraction(tool, "mcp");

    // Determine if we have any includable params (excluding ProgressReporter/CancellationToken)
    let has_includable_params = tool
        .params
        .iter()
        .any(|p| crate::codegen::validation::should_include_param(p, "mcp"));

    // If there are includable params, assert params is a JSON object and bind it
    let object_assertion = if has_includable_params {
        quote! {
            let params = match params {
                ::serde_json::Value::Object(map) => map,
                _ => {
                    return ::std::result::Result::Err(
                        ::universal_tool_core::error::ToolError::new(
                            ::universal_tool_core::error::ErrorCode::InvalidArgument,
                            "Parameters must be a JSON object"
                        )
                    );
                }
            };
        }
    } else {
        quote! {}
    };

    // Generate method call (now uses local variables instead of params.#field)
    let method_call = generate_method_call(tool);

    quote! {
        #tool_name => {
            #object_assertion
            #( #param_extractions )*

            #method_call
        }
    }
}

/// Generates the method call with proper parameter passing and result serialization
fn generate_method_call(tool: &ToolDef) -> TokenStream {
    // Build parameter list, handling special MCP parameters
    // For normal params, use the local variable bound by per-field extraction
    let param_args: Vec<_> = tool
        .params
        .iter()
        .map(|param| {
            let name = &param.name;
            let ty = &param.ty;
            let ty_str = quote!(#ty).to_string();

            if ty_str.contains("ProgressReporter") {
                // For now, pass None for progress reporter
                // TODO: In the future, could accept progress_token in params and create reporter
                quote! { None }
            } else if ty_str.contains("CancellationToken") {
                // Create a new cancellation token
                // TODO: In the future, could get from MCP request context
                quote! { ::universal_tool_core::mcp::CancellationToken::new() }
            } else {
                // Regular parameter - use local variable binding
                quote! { #name }
            }
        })
        .collect();

    // Use the shared function to generate the method call with proper async/sync handling
    let method_call =
        crate::codegen::shared::generate_normalized_method_call(tool, quote! { self }, param_args);

    quote! {
        let result = #method_call?;
        ::std::result::Result::Ok(::serde_json::to_value(&result)?)
    }
}

/// Generates a match arm for the new MCP dispatch method with output mode support
fn generate_tool_match_arm_mcp(tool: &ToolDef) -> TokenStream {
    let tool_name = &tool.tool_name;

    // Generate per-field extraction bindings for MCP
    let param_extractions = crate::codegen::validation::generate_params_extraction(tool, "mcp");

    // Determine if we have any includable params (excluding ProgressReporter/CancellationToken)
    let has_includable_params = tool
        .params
        .iter()
        .any(|p| crate::codegen::validation::should_include_param(p, "mcp"));

    // If there are includable params, assert params is a JSON object and bind it
    let object_assertion = if has_includable_params {
        quote! {
            let params = match params {
                ::serde_json::Value::Object(map) => map,
                _ => {
                    return ::std::result::Result::Err(
                        ::universal_tool_core::error::ToolError::new(
                            ::universal_tool_core::error::ErrorCode::InvalidArgument,
                            "Parameters must be a JSON object"
                        )
                    );
                }
            };
        }
    } else {
        quote! {}
    };

    // Build parameter args list using local bindings
    let param_args: Vec<_> = tool
        .params
        .iter()
        .map(|param| {
            let name = &param.name;
            let ty = &param.ty;
            let ty_str = quote!(#ty).to_string();

            if ty_str.contains("ProgressReporter") {
                quote! { None }
            } else if ty_str.contains("CancellationToken") {
                quote! { ::universal_tool_core::mcp::CancellationToken::new() }
            } else {
                quote! { #name }
            }
        })
        .collect();

    let method_call =
        crate::codegen::shared::generate_normalized_method_call(tool, quote! { self }, param_args);

    // Determine output mode at codegen time
    let output_mode_tokens = if let Some(mcp_config) = &tool.metadata.mcp_config {
        if matches!(
            mcp_config.output_mode,
            Some(crate::model::McpOutputMode::Text)
        ) {
            // TEXT mode: require McpFormatter at compile time
            quote! {
                let result = #method_call?;
                let text = ::universal_tool_core::mcp::McpFormatter::mcp_format_text(&result);
                ::std::result::Result::Ok(::universal_tool_core::mcp::McpOutput::Text(text))
            }
        } else {
            // JSON mode
            quote! {
                let result = #method_call?;
                let val = ::serde_json::to_value(&result)?;
                ::std::result::Result::Ok(::universal_tool_core::mcp::McpOutput::Json(val))
            }
        }
    } else {
        // Default to JSON
        quote! {
            let result = #method_call?;
            let val = ::serde_json::to_value(&result)?;
            ::std::result::Result::Ok(::universal_tool_core::mcp::McpOutput::Json(val))
        }
    };

    quote! {
        #tool_name => {
            #object_assertion
            #( #param_extractions )*
            #output_mode_tokens
        }
    }
}

/// Generates the get_mcp_tools() method for tool discovery
fn generate_mcp_tools_method(router: &RouterDef) -> TokenStream {
    let tool_definitions = router.tools.iter().map(generate_tool_definition);

    quote! {
        /// Returns tool definitions for MCP discovery
        /// Users can use this to implement list_tools in their ServerHandler
        pub fn get_mcp_tools(&self) -> ::std::vec::Vec<::serde_json::Value> {
            ::std::vec![
                #(#tool_definitions),*
            ]
        }
    }
}

/// Generates a tool definition JSON for MCP discovery
fn generate_tool_definition(tool: &ToolDef) -> TokenStream {
    let name = &tool.tool_name;
    let description = &tool.metadata.description;

    // Generate input schema
    let schema = generate_tool_schema(tool);

    // Build the tool definition based on whether we have annotations
    if let Some(mcp_config) = &tool.metadata.mcp_config {
        let mut has_annotations = false;
        let mut annotation_fields = vec![];

        if let Some(v) = mcp_config.annotations.read_only_hint {
            has_annotations = true;
            annotation_fields.push(quote! { "readOnlyHint": #v });
        }
        if let Some(v) = mcp_config.annotations.destructive_hint {
            has_annotations = true;
            annotation_fields.push(quote! { "destructiveHint": #v });
        }
        if let Some(v) = mcp_config.annotations.idempotent_hint {
            has_annotations = true;
            annotation_fields.push(quote! { "idempotentHint": #v });
        }
        if let Some(v) = mcp_config.annotations.open_world_hint {
            has_annotations = true;
            annotation_fields.push(quote! { "openWorldHint": #v });
        }

        if has_annotations {
            quote! {
                {
                    let schema = #schema;
                    let mut tool_def = ::serde_json::json!({
                        "name": #name,
                        "description": #description,
                        "inputSchema": schema
                    });
                    tool_def["annotations"] = ::serde_json::json!({
                        #(#annotation_fields),*
                    });
                    tool_def
                }
            }
        } else {
            quote! {
                {
                    let schema = #schema;
                    ::serde_json::json!({
                        "name": #name,
                        "description": #description,
                        "inputSchema": schema
                    })
                }
            }
        }
    } else {
        quote! {
            {
                let schema = #schema;
                ::serde_json::json!({
                    "name": #name,
                    "description": #description,
                    "inputSchema": schema
                })
            }
        }
    }
}

/// Generates JSON schema for a single parameter using schemars
fn generate_param_schema(param: &ParamDef) -> TokenStream {
    let param_type = &param.ty;
    let description = &param.metadata.description.as_deref().unwrap_or("");

    quote! {
        {
            let mut settings = ::universal_tool_core::schemars::r#gen::SchemaSettings::draft07();
            settings.inline_subschemas = true;
            let mut schema_gen = settings.into_generator();
            let schema = <#param_type as ::universal_tool_core::JsonSchema>::json_schema(&mut schema_gen);
            let mut json_schema = ::serde_json::to_value(&schema).unwrap_or_else(|_| ::serde_json::json!({"type": "string"}));
            if let ::serde_json::Value::Object(ref mut map) = json_schema {
                if !#description.is_empty() {
                    map.insert("description".to_string(), ::serde_json::Value::String(#description.to_string()));
                }
            }
            json_schema
        }
    }
}

/// Generates JSON schema for tool parameters
fn generate_tool_schema(tool: &ToolDef) -> TokenStream {
    // Filter out special MCP parameters from schema
    let schema_params: Vec<_> = tool
        .params
        .iter()
        .filter(|param| validation::should_include_param(param, "mcp"))
        .collect();

    if schema_params.is_empty() {
        // No parameters - empty object schema
        quote! {
            ::serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        }
    } else {
        // Generate properties using schemars
        let param_schemas: Vec<_> = schema_params
            .iter()
            .map(|param| {
                let name = &param.name.to_string();
                let schema = generate_param_schema(param);
                let required = !is_optional_type(&param.ty);
                quote! {
                    properties.insert(#name.to_string(), #schema);
                    if #required {
                        required.push(#name.to_string());
                    }
                }
            })
            .collect();

        quote! {
            {
                let mut properties = ::serde_json::Map::new();
                let mut required = Vec::new();

                #(#param_schemas)*

                ::serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": required
                })
            }
        }
    }
}

/// Generates get_mcp_server_info() returning (name, version) with router-level precedence
fn generate_mcp_server_info_method(router: &RouterDef) -> TokenStream {
    // Determine default name from struct type last segment
    let default_name = router
        .struct_type
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_else(|| "tools".to_string());

    // Compute name: RouterMcpConfig.name or default struct name
    let name_val: String = router
        .metadata
        .mcp_config
        .as_ref()
        .and_then(|c| c.name.as_ref())
        .cloned()
        .unwrap_or(default_name);

    // Compute version tokens:
    // - If RouterMcpConfig.version set, embed that literal.
    // - Otherwise, embed env!("CARGO_PKG_VERSION") at use-site crate.
    let version_tokens: TokenStream = if let Some(cfg) = &router.metadata.mcp_config {
        if let Some(ver) = &cfg.version {
            let ver_lit = ver.clone();
            quote! { #ver_lit.to_string() }
        } else {
            quote! { env!("CARGO_PKG_VERSION").to_string() }
        }
    } else {
        quote! { env!("CARGO_PKG_VERSION").to_string() }
    };

    quote! {
        /// Returns (server name, server version) for MCP initialize response serverInfo
        pub fn get_mcp_server_info(&self) -> (String, String) {
            let name = #name_val.to_string();
            let version = #version_tokens;
            (name, version)
        }
    }
}
