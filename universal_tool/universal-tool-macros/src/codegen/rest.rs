//! REST API code generation for Universal Tool Framework
//!
//! This module generates `create_rest_router()` and `get_openapi_spec()` methods
//! that integrate tools with axum-based REST APIs.

use crate::codegen::shared::{to_kebab_case, to_pascal_case};
use crate::model::{HttpMethod, ParamSource, RouterDef, ToolDef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

/// Generates all REST-related methods for a router
/// Returns a tuple of (module_code, method_code) to allow proper separation
pub fn generate_rest_methods_split(router: &RouterDef) -> (TokenStream, TokenStream) {
    let struct_type = &router.struct_type;

    // Generate a unique module name based on the struct type
    let module_name = syn::Ident::new(
        &format!(
            "__utf_rest_generated_{}",
            struct_type
                .segments
                .last()
                .unwrap()
                .ident
                .to_string()
                .to_lowercase()
        ),
        struct_type.span(),
    );

    let create_router_method = generate_create_rest_router(router, &module_name);
    let openapi_method = generate_get_openapi_spec(router);
    let param_structs = generate_param_structs(router);

    let module_code = quote! {
        mod #module_name {
            use super::*;
            use ::serde::{Serialize, Deserialize};

            #param_structs
        }
    };

    // Wrap methods in an impl block
    let method_code = quote! {
        impl #struct_type {
            #create_router_method

            #openapi_method
        }
    };

    (module_code, method_code)
}

/// Generates all REST-related methods for a router (legacy version for compatibility)
#[allow(dead_code)] // Part of incomplete OpenAPI feature
pub fn generate_rest_methods(router: &RouterDef) -> TokenStream {
    let (module_code, method_code) = generate_rest_methods_split(router);
    quote! {
        #module_code
        #method_code
    }
}

/// Generates the create_rest_router() method
fn generate_create_rest_router(router: &RouterDef, module_name: &syn::Ident) -> TokenStream {
    let routes = router
        .tools
        .iter()
        .map(|tool| generate_tool_route(tool, module_name));

    // TODO(2): Apply base_path from router metadata to prefix all routes
    // Currently routes are generated without the base_path prefix
    let _prefix = router.metadata.base_path.as_deref().unwrap_or("/api");

    quote! {
        /// Creates an axum::Router with routes for all tools
        ///
        /// This is an associated function that takes the application state as an Arc.
        /// Usage: `let app = MyStruct::create_rest_router(Arc::new(my_instance));`
        ///
        /// Users integrate this router into their web server however they need
        pub fn create_rest_router(state: ::std::sync::Arc<Self>) -> ::universal_tool_core::rest::Router {
            use ::universal_tool_core::rest::{Router, response::IntoResponse};

            Router::new()
                #( #routes )*
                .with_state(state)
        }
    }
}

/// Generates parameter structs for each tool
fn generate_param_structs(router: &RouterDef) -> TokenStream {
    router
        .tools
        .iter()
        .map(|tool| {
            let struct_name = get_params_struct_name(tool);

            // Only generate struct if there are body parameters
            let body_params: Vec<_> = tool
                .params
                .iter()
                .filter(|p| matches!(p.source, ParamSource::Body))
                .collect();

            if body_params.is_empty() {
                quote! {}
            } else {
                let fields = body_params.iter().map(|param| {
                    let name = &param.name;
                    let ty = &param.ty;
                    let doc = param
                        .metadata
                        .description
                        .as_deref()
                        .unwrap_or("")
                        .to_string();

                    quote! {
                        #[doc = #doc]
                        pub #name: #ty
                    }
                });

                quote! {
                    #[derive(Debug, Clone, Serialize, Deserialize)]
                    pub struct #struct_name {
                        #( #fields ),*
                    }
                }
            }
        })
        .collect()
}

/// Generates a route for a single tool
fn generate_tool_route(tool: &ToolDef, module_name: &syn::Ident) -> TokenStream {
    let method = determine_http_method(tool);
    let path = generate_route_path(tool);
    let _handler_name = get_handler_name(tool);

    // Generate the handler function
    let handler = generate_handler(tool, module_name);

    // Generate the route registration
    let route_method = match method {
        HttpMethod::Get => quote! { ::universal_tool_core::rest::routing::get },
        HttpMethod::Post => quote! { ::universal_tool_core::rest::routing::post },
        HttpMethod::Put => quote! { ::universal_tool_core::rest::routing::put },
        HttpMethod::Delete => quote! { ::universal_tool_core::rest::routing::delete },
        HttpMethod::Patch => quote! { ::universal_tool_core::rest::routing::patch },
    };

    quote! {
        .route(#path, #route_method(#handler))
    }
}

/// Generates the handler closure for a tool
fn generate_handler(tool: &ToolDef, module_name: &syn::Ident) -> TokenStream {
    let has_body_params = tool
        .params
        .iter()
        .any(|p| matches!(p.source, ParamSource::Body));

    // Generate parameter extraction
    let param_extractors = generate_param_extractors(tool);

    // Generate the handler body with inline error handling
    let error_handling = quote! {
        let status = match e.code {
            ::universal_tool_core::error::ErrorCode::BadRequest => ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
            ::universal_tool_core::error::ErrorCode::InvalidArgument => ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
            ::universal_tool_core::error::ErrorCode::NotFound => ::universal_tool_core::rest::StatusCode::NOT_FOUND,
            ::universal_tool_core::error::ErrorCode::PermissionDenied => ::universal_tool_core::rest::StatusCode::FORBIDDEN,
            ::universal_tool_core::error::ErrorCode::Internal => ::universal_tool_core::rest::StatusCode::INTERNAL_SERVER_ERROR,
            ::universal_tool_core::error::ErrorCode::Timeout => ::universal_tool_core::rest::StatusCode::REQUEST_TIMEOUT,
            ::universal_tool_core::error::ErrorCode::Conflict => ::universal_tool_core::rest::StatusCode::CONFLICT,
            ::universal_tool_core::error::ErrorCode::NetworkError => ::universal_tool_core::rest::StatusCode::SERVICE_UNAVAILABLE,
            ::universal_tool_core::error::ErrorCode::ExternalServiceError => ::universal_tool_core::rest::StatusCode::BAD_GATEWAY,
            ::universal_tool_core::error::ErrorCode::ExecutionFailed => ::universal_tool_core::rest::StatusCode::INTERNAL_SERVER_ERROR,
            ::universal_tool_core::error::ErrorCode::SerializationError => ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
            ::universal_tool_core::error::ErrorCode::IoError => ::universal_tool_core::rest::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, ::universal_tool_core::rest::Json(::serde_json::json!({
            "error": e.to_string(),
            "code": format!("{:?}", e.code),
        }))).into_response()
    };

    // Build the arguments vector for the method call
    let method_args_vec: Vec<TokenStream> = tool
        .params
        .iter()
        .map(|param| {
            let name = &param.name;
            match param.source {
                ParamSource::Body => quote! { params.#name },
                _ => quote! { #name },
            }
        })
        .collect();

    // Use the shared function to generate the method call with proper async/sync handling
    let method_call = crate::codegen::shared::generate_normalized_method_call(
        tool,
        quote! { state },
        method_args_vec,
    );

    if has_body_params {
        let params_struct = get_params_struct_name(tool);
        quote! {
            |::universal_tool_core::rest::State(state): ::universal_tool_core::rest::State<::std::sync::Arc<Self>>#param_extractors,
             ::universal_tool_core::rest::Json(params): ::universal_tool_core::rest::Json<#module_name::#params_struct>| async move {
                match #method_call {
                    Ok(result) => (::universal_tool_core::rest::StatusCode::OK, ::universal_tool_core::rest::Json(result)).into_response(),
                    Err(e) => { #error_handling },
                }
            }
        }
    } else {
        quote! {
            |::universal_tool_core::rest::State(state): ::universal_tool_core::rest::State<::std::sync::Arc<Self>>#param_extractors| async move {
                match #method_call {
                    Ok(result) => (::universal_tool_core::rest::StatusCode::OK, ::universal_tool_core::rest::Json(result)).into_response(),
                    Err(e) => { #error_handling },
                }
            }
        }
    }
}

/// Generates parameter extractors for non-body parameters
fn generate_param_extractors(tool: &ToolDef) -> TokenStream {
    let extractors = tool.params.iter()
        .filter(|p| !matches!(p.source, ParamSource::Body))
        .map(|param| {
            let name = &param.name;
            let ty = &param.ty;

            match param.source {
                ParamSource::Path => quote! { , ::universal_tool_core::rest::Path(#name): ::universal_tool_core::rest::Path<#ty> },
                ParamSource::Query => quote! { , ::universal_tool_core::rest::Query(#name): ::universal_tool_core::rest::Query<#ty> },
                _ => quote! {},
            }
        });

    quote! { #( #extractors )* }
}

/// Generates the get_openapi_spec() method
fn generate_get_openapi_spec(_router: &RouterDef) -> TokenStream {
    // TODO(3): Implement full OpenAPI generation when openapi feature is enabled
    // This would use utoipa macros to generate a proper OpenAPI spec with schemas
    // For now, return a placeholder that lets users know the feature is not enabled
    quote! {
        /// Returns OpenAPI documentation for all tools
        ///
        /// Note: Full OpenAPI generation requires the 'openapi' feature to be enabled:
        /// ```toml
        /// universal-tool-core = { version = "0.1", features = ["rest", "openapi"] }
        /// ```
        pub fn get_openapi_spec(&self) -> String {
            "OpenAPI generation requires the 'openapi' feature to be enabled".to_string()
        }
    }
}

/// Determines the HTTP method for a tool
fn determine_http_method(tool: &ToolDef) -> HttpMethod {
    // Check if REST config specifies a method
    if let Some(rest_config) = &tool.metadata.rest_config {
        return rest_config.method;
    }

    // Use smart defaults based on method name
    let name = tool.method_name.to_string();
    if name.starts_with("get") || name.starts_with("list") || name.starts_with("find") {
        HttpMethod::Get
    } else if name.starts_with("create") || name.starts_with("add") || name.starts_with("new") {
        HttpMethod::Post
    } else if name.starts_with("update") || name.starts_with("modify") || name.starts_with("set") {
        HttpMethod::Put
    } else if name.starts_with("delete") || name.starts_with("remove") {
        HttpMethod::Delete
    } else if name.starts_with("patch") {
        HttpMethod::Patch
    } else {
        // Default to POST for other operations
        HttpMethod::Post
    }
}

/// Generates the route path for a tool
fn generate_route_path(tool: &ToolDef) -> String {
    // Check if REST config specifies a path
    if let Some(rest_config) = &tool.metadata.rest_config
        && let Some(path) = &rest_config.path
    {
        return path.clone();
    }

    // Generate path based on tool name
    let base_path = format!("/{}", to_kebab_case(&tool.tool_name));

    // Add path parameters
    let mut path = base_path;
    for param in &tool.params {
        if matches!(param.source, ParamSource::Path) {
            path.push_str(&format!("/:{}", param.name));
        }
    }

    path
}

/// Gets the name for the params struct for a tool
fn get_params_struct_name(tool: &ToolDef) -> syn::Ident {
    let name = format!("{}Params", to_pascal_case(&tool.tool_name));
    syn::Ident::new(&name, tool.method_name.span())
}

/// Gets the handler function name for a tool
fn get_handler_name(tool: &ToolDef) -> syn::Ident {
    let name = format!("handle_rest_{}", tool.method_name);
    syn::Ident::new(&name, tool.method_name.span())
}

/// Generate OpenAPI schemas for parameter structs
#[allow(dead_code)] // Will be used when OpenAPI feature is completed
fn generate_openapi_schemas(router: &RouterDef) -> TokenStream {
    let schemas = router
        .tools
        .iter()
        .filter(|tool| {
            tool.params
                .iter()
                .any(|p| matches!(p.source, ParamSource::Body))
        })
        .map(|_tool| {
            quote! {
                // ToSchema is already derived above
            }
        });

    quote! { #( #schemas )* }
}

/// Generate OpenAPI path documentation
#[allow(dead_code)] // Will be used when OpenAPI feature is completed
fn generate_openapi_paths(router: &RouterDef) -> TokenStream {
    let paths = router.tools.iter().map(|tool| {
        let fn_name = syn::Ident::new(
            &format!("__openapi_path_{}", tool.method_name),
            tool.method_name.span()
        );
        let path = generate_route_path(tool);
        let method = determine_http_method(tool);
        let method_str = format!("{method:?}").to_lowercase();
        let description = &tool.metadata.description;

        // Determine request body
        let has_body = tool.params.iter().any(|p| matches!(p.source, ParamSource::Body));
        let request_body = if has_body {
            let struct_name = get_params_struct_name(tool);
            quote! { request_body = #struct_name, }
        } else {
            quote! {}
        };

        // Extract the success type from Result<T, E>
        let return_type = &tool.return_type;

        quote! {
            // OpenAPI paths are always generated when REST is enabled
            #[::utoipa::path(
                #method_str,
                path = #path,
                #request_body
                responses(
                    (status = 200, description = #description, body = #return_type),
                    (status = 400, description = "Bad request", body = ::universal_tool_core::error::ToolError),
                    (status = 500, description = "Internal server error", body = ::universal_tool_core::error::ToolError)
                )
            )]
            async fn #fn_name() {}
        }
    });

    quote! { #( #paths )* }
}
