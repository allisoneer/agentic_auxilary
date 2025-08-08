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
    let tools_method = generate_mcp_tools_method(router);

    quote! {
        impl #struct_type {
            #dispatch_method

            #tools_method
        }
    }
}

/// Generates the handle_mcp_call() method for dispatching MCP tool calls
fn generate_mcp_dispatch_method(router: &RouterDef) -> TokenStream {
    let match_arms: Vec<_> = router
        .tools
        .iter()
        .map(|tool| generate_tool_match_arm(tool))
        .collect();

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

/// Generates a match arm for a single tool
fn generate_tool_match_arm(tool: &ToolDef) -> TokenStream {
    let tool_name = &tool.tool_name;
    let _method_ident = &tool.method_name;

    // Create parameter struct definition
    let params_struct = generate_params_struct(tool);

    // Generate method call with proper parameter passing
    let method_call = generate_method_call(tool);

    quote! {
        #tool_name => {
            #params_struct

            let params: __Params = ::serde_json::from_value(params)?;
            #method_call
        }
    }
}

/// Generates inline parameter struct for deserialization
fn generate_params_struct(tool: &ToolDef) -> TokenStream {
    // Filter out special MCP parameters that come from context
    let deserializable_params: Vec<_> = tool
        .params
        .iter()
        .filter(|param| validation::should_include_param(param, "mcp"))
        .collect();

    if deserializable_params.is_empty() {
        // No parameters - generate empty struct
        quote! {
            #[derive(::serde::Deserialize)]
            struct __Params {}
        }
    } else {
        let fields = deserializable_params.iter().map(|param| {
            let name = &param.name;
            let ty = &param.ty;
            quote! {
                #name: #ty
            }
        });

        quote! {
            #[derive(::serde::Deserialize)]
            struct __Params {
                #(#fields),*
            }
        }
    }
}

/// Generates the method call with proper parameter passing and result serialization
fn generate_method_call(tool: &ToolDef) -> TokenStream {
    // Build parameter list, handling special MCP parameters
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
                // Regular parameter from deserialized params
                quote! { params.#name }
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

/// Generates the get_mcp_tools() method for tool discovery
fn generate_mcp_tools_method(router: &RouterDef) -> TokenStream {
    let tool_definitions = router
        .tools
        .iter()
        .map(|tool| generate_tool_definition(tool));

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
            let mut schema_gen = ::universal_tool_core::schemars::r#gen::SchemaGenerator::default();
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
