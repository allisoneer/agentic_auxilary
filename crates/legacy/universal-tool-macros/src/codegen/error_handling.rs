//! Shared error handling code generation utilities
//!
//! This module provides consistent error handling across all interfaces.

#![allow(dead_code)]

use proc_macro2::TokenStream;
use quote::quote;

/// Generate error handling for unknown tool/command
pub fn generate_unknown_tool_error(tool_name: &str, interface: &str) -> TokenStream {
    match interface {
        "cli" => quote! {
            eprintln!("Error: Unknown command '{}'", #tool_name);
            eprintln!("Try '--help' for more information.");
            return Err(::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::NotFound,
                format!("Unknown command: {}", #tool_name)
            ));
        },
        "rest" => quote! {
            return Ok((
                ::universal_tool_core::rest::StatusCode::NOT_FOUND,
                ::universal_tool_core::rest::Json(::serde_json::json!({
                    "error": format!("Unknown endpoint: {}", #tool_name),
                    "code": "NotFound"
                }))
            ).into_response());
        },
        "mcp" => quote! {
            return Err(::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::NotFound,
                format!("Unknown method: {}", #tool_name)
            ));
        },
        _ => quote! {
            return Err(::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::NotFound,
                format!("Unknown tool: {}", #tool_name)
            ));
        },
    }
}

/// Generate consistent error response for tool errors
pub fn generate_tool_error_response(error_var: &str, interface: &str) -> TokenStream {
    let err = quote::format_ident!("{}", error_var);

    match interface {
        "cli" => quote! {
            eprintln!("Error: {}", #err);
            if let Some(details) = #err.details {
                for (key, value) in details {
                    eprintln!("  {}: {}", key, value);
                }
            }
            std::process::exit(match #err.code {
                ::universal_tool_core::prelude::ErrorCode::BadRequest => 2,
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument => 2,
                ::universal_tool_core::prelude::ErrorCode::NotFound => 3,
                ::universal_tool_core::prelude::ErrorCode::PermissionDenied => 4,
                ::universal_tool_core::prelude::ErrorCode::Conflict => 5,
                ::universal_tool_core::prelude::ErrorCode::NetworkError => 6,
                ::universal_tool_core::prelude::ErrorCode::ExternalServiceError => 7,
                ::universal_tool_core::prelude::ErrorCode::ExecutionFailed => 8,
                ::universal_tool_core::prelude::ErrorCode::SerializationError => 9,
                ::universal_tool_core::prelude::ErrorCode::IoError => 10,
                _ => 1,
            });
        },
        "rest" => {
            // Use the existing REST error mapping from rest.rs
            quote! {
                let status = match #err.code {
                    ::universal_tool_core::error::ErrorCode::BadRequest => ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
                    ::universal_tool_core::error::ErrorCode::InvalidArgument => ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
                    ::universal_tool_core::error::ErrorCode::NotFound => ::universal_tool_core::rest::StatusCode::NOT_FOUND,
                    ::universal_tool_core::error::ErrorCode::PermissionDenied => ::universal_tool_core::rest::StatusCode::FORBIDDEN,
                    ::universal_tool_core::error::ErrorCode::Internal => ::universal_tool_core::rest::StatusCode::INTERNAL_SERVER_ERROR,
                    ::universal_tool_core::error::ErrorCode::Timeout => ::universal_tool_core::rest::StatusCode::REQUEST_TIMEOUT,
                    ::universal_tool_core::error::ErrorCode::Conflict => ::universal_tool_core::rest::StatusCode::CONFLICT,
                    ::universal_tool_core::error::ErrorCode::NetworkError => ::universal_tool_core::rest::StatusCode::BAD_GATEWAY,
                    ::universal_tool_core::error::ErrorCode::ExternalServiceError => ::universal_tool_core::rest::StatusCode::SERVICE_UNAVAILABLE,
                    ::universal_tool_core::error::ErrorCode::ExecutionFailed => ::universal_tool_core::rest::StatusCode::INTERNAL_SERVER_ERROR,
                    ::universal_tool_core::error::ErrorCode::SerializationError => ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
                    ::universal_tool_core::error::ErrorCode::IoError => ::universal_tool_core::rest::StatusCode::INTERNAL_SERVER_ERROR,
                };
                let details = #err.details.as_ref()
                    .map(|d| ::serde_json::to_value(d).ok())
                    .flatten();
                (status, ::universal_tool_core::rest::Json(::serde_json::json!({
                    "error": #err.to_string(),
                    "code": format!("{:?}", #err.code),
                    "details": details
                }))).into_response()
            }
        }
        "mcp" => quote! {
            // MCP errors are already converted via From<ToolError> for McpErrorData
            Err(#err)
        },
        _ => quote! {
            Err(#err)
        },
    }
}

/// Generate validation error for missing required parameter
pub fn generate_missing_param_error(param_name: &str, interface: &str) -> TokenStream {
    let error_msg = format!("Missing required parameter: {param_name}");

    match interface {
        "cli" => quote! {
            ::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                #error_msg
            )
        },
        "rest" => quote! {
            return Ok((
                ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
                ::universal_tool_core::rest::Json(::serde_json::json!({
                    "error": #error_msg,
                    "code": "InvalidArgument",
                    "parameter": #param_name
                }))
            ).into_response());
        },
        "mcp" => quote! {
            ::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                #error_msg
            )
        },
        _ => quote! {
            ::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                #error_msg
            )
        },
    }
}

/// Generate parsing error for invalid parameter value
pub fn generate_parse_error(param_name: &str, expected_type: &str, interface: &str) -> TokenStream {
    let error_msg =
        format!("Invalid value for parameter '{param_name}'. Expected: {expected_type}");

    match interface {
        "cli" => quote! {
            ::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                #error_msg
            )
        },
        "rest" => quote! {
            return Ok((
                ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
                ::universal_tool_core::rest::Json(::serde_json::json!({
                    "error": #error_msg,
                    "code": "InvalidArgument",
                    "parameter": #param_name,
                    "expected_type": #expected_type
                }))
            ).into_response());
        },
        "mcp" => quote! {
            ::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                #error_msg
            )
        },
        _ => quote! {
            ::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                #error_msg
            )
        },
    }
}

/// Generate consistent error for validation failures
pub fn generate_validation_error(validation_msg: &str, interface: &str) -> TokenStream {
    match interface {
        "cli" => quote! {
            eprintln!("Validation error: {}", #validation_msg);
            return Err(::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                format!("Validation error: {}", #validation_msg)
            ));
        },
        "rest" => quote! {
            return Ok((
                ::universal_tool_core::rest::StatusCode::BAD_REQUEST,
                ::universal_tool_core::rest::Json(::serde_json::json!({
                    "error": format!("Validation error: {}", #validation_msg),
                    "code": "InvalidArgument",
                    "validation_error": #validation_msg
                }))
            ).into_response());
        },
        "mcp" => quote! {
            return Err(::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                format!("Validation error: {}", #validation_msg)
            ));
        },
        _ => quote! {
            return Err(::universal_tool_core::prelude::ToolError::new(
                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                format!("Validation error: {}", #validation_msg)
            ));
        },
    }
}
