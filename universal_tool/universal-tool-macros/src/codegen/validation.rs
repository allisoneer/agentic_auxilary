//! Unified validation utilities for UTF code generation
//!
//! This module provides common validation patterns that work across all interfaces
//! (CLI, REST, MCP) to ensure consistent parameter handling and error messages.

use crate::codegen::error_handling;
use crate::codegen::shared::{
    is_bool_type, is_custom_struct_type, is_hashmap_type, is_optional_type, is_vec_type,
};
use crate::model::{ParamDef, ToolDef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Type, parse_str};

/// Generates code to extract a parameter with appropriate validation
/// This unifies the extraction logic across all interfaces
pub fn generate_param_extraction(param: &ParamDef, interface: &str) -> TokenStream {
    let param_ident = &param.name;

    match interface {
        "cli" => generate_cli_param_extraction(param),
        "rest" => {
            // REST params come from the generated params struct
            quote! { let #param_ident = params.#param_ident; }
        }
        "mcp" => generate_mcp_param_extraction(param),
        _ => quote! {},
    }
}

/// Generates CLI-specific parameter extraction with validation
fn generate_cli_param_extraction(param: &ParamDef) -> TokenStream {
    let param_name = &param.name.to_string();
    let param_ident = &param.name;
    let param_type = &param.ty;

    if is_custom_struct_type(&param.ty) {
        // Custom structs are passed as JSON strings
        let missing_err = error_handling::generate_missing_param_error(param_name, "cli");
        let parse_err = error_handling::generate_parse_error(param_name, "JSON object", "cli");

        if is_optional_type(&param.ty) {
            quote! {
                let #param_ident: #param_type = if let Some(json_str) = sub_matches.get_one::<String>(#param_name) {
                    Some(::serde_json::from_str(json_str)
                        .map_err(|e| #parse_err)?)
                } else {
                    None
                };
            }
        } else {
            quote! {
                let #param_ident: #param_type = {
                    let json_str = sub_matches.get_one::<String>(#param_name)
                        .ok_or_else(|| #missing_err)?;
                    ::serde_json::from_str(json_str)
                        .map_err(|e| {
                            ::universal_tool_core::prelude::ToolError::new(
                                ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                                format!("Failed to parse JSON for {}: {}", #param_name, e)
                            )
                        })?
                };
            }
        }
    } else if is_vec_type(&param.ty) {
        // Vec types use get_many
        quote! {
            let #param_ident: #param_type = sub_matches.get_many::<String>(#param_name)
                .map(|values| values.map(|s| s.clone()).collect())
                .unwrap_or_else(Vec::new);
        }
    } else if is_hashmap_type(&param.ty) {
        // HashMap types parse key=value pairs
        let parse_err = error_handling::generate_parse_error(param_name, "key=value format", "cli");

        // Extract the value type from HashMap<K, V>
        let value_type = extract_hashmap_value_type(&param.ty);
        let value_type_str = quote!(#value_type).to_string();

        // Generate parsing code based on the value type
        let value_parse = match value_type_str.as_str() {
            "String" => quote! { parts[1].to_string() },
            "i32" => quote! {
                parts[1].parse::<i32>().map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid i32 value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
            "i64" => quote! {
                parts[1].parse::<i64>().map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid i64 value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
            "u32" => quote! {
                parts[1].parse::<u32>().map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid u32 value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
            "u64" => quote! {
                parts[1].parse::<u64>().map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid u64 value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
            "f32" => quote! {
                parts[1].parse::<f32>().map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid f32 value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
            "f64" => quote! {
                parts[1].parse::<f64>().map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid f64 value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
            "bool" => quote! {
                parts[1].parse::<bool>().map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid bool value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
            _ => quote! {
                ::serde_json::from_str(parts[1]).map_err(|_| {
                    ::universal_tool_core::prelude::ToolError::new(
                        ::universal_tool_core::prelude::ErrorCode::InvalidArgument,
                        format!("Invalid JSON value for {}: {}", #param_name, parts[1])
                    )
                })?
            },
        };

        quote! {
            let #param_ident: #param_type = {
                let mut map = std::collections::HashMap::new();
                if let Some(values) = sub_matches.get_many::<String>(#param_name) {
                    for kv in values {
                        let parts: Vec<&str> = kv.splitn(2, '=').collect();
                        if parts.len() != 2 {
                            return Err(#parse_err);
                        }
                        let value = #value_parse;
                        map.insert(parts[0].to_string(), value);
                    }
                }
                map
            };
        }
    } else {
        // Simple types
        let missing_err = error_handling::generate_missing_param_error(param_name, "cli");

        if is_optional_type(&param.ty) {
            // For Option<T>, we need to handle the inner type
            let inner_type = extract_option_inner_type(&param.ty);
            let type_str = quote!(#inner_type).to_string();

            // Handle different inner types
            match type_str.as_str() {
                "String" => quote! {
                    let #param_ident: #param_type = sub_matches.get_one::<String>(#param_name).cloned();
                },
                "i32" | "i64" | "u32" | "u64" | "f32" | "f64" => quote! {
                    let #param_ident: #param_type = sub_matches.get_one::<#inner_type>(#param_name).cloned();
                },
                _ => quote! {
                    let #param_ident: #param_type = sub_matches.get_one::<String>(#param_name)
                        .and_then(|s| ::serde_json::from_str(s).ok());
                },
            }
        } else if is_bool_type(&param.ty) {
            quote! {
                let #param_ident = sub_matches.get_flag(#param_name);
            }
        } else {
            // For non-optional types, we need to handle the type properly
            let type_str = quote!(#param_type).to_string();

            match type_str.as_str() {
                "String" => quote! {
                    let #param_ident = sub_matches.get_one::<String>(#param_name)
                        .ok_or_else(|| #missing_err)?
                        .clone();
                },
                "i32" => quote! {
                    let #param_ident = sub_matches.get_one::<i32>(#param_name)
                        .ok_or_else(|| #missing_err)?
                        .clone();
                },
                "i64" => quote! {
                    let #param_ident = sub_matches.get_one::<i64>(#param_name)
                        .ok_or_else(|| #missing_err)?
                        .clone();
                },
                "u32" => quote! {
                    let #param_ident = sub_matches.get_one::<u32>(#param_name)
                        .ok_or_else(|| #missing_err)?
                        .clone();
                },
                "u64" => quote! {
                    let #param_ident = sub_matches.get_one::<u64>(#param_name)
                        .ok_or_else(|| #missing_err)?
                        .clone();
                },
                "f32" => quote! {
                    let #param_ident = sub_matches.get_one::<f32>(#param_name)
                        .ok_or_else(|| #missing_err)?
                        .clone();
                },
                "f64" => quote! {
                    let #param_ident = sub_matches.get_one::<f64>(#param_name)
                        .ok_or_else(|| #missing_err)?
                        .clone();
                },
                _ => {
                    // For other types, parse as JSON
                    let parse_err =
                        error_handling::generate_parse_error(param_name, "value", "cli");
                    quote! {
                        let #param_ident: #param_type = sub_matches.get_one::<String>(#param_name)
                            .ok_or_else(|| #missing_err)?
                            .parse()
                            .map_err(|_| #parse_err)?;
                    }
                }
            }
        }
    }
}

/// Generates MCP-specific parameter extraction with validation
fn generate_mcp_param_extraction(param: &ParamDef) -> TokenStream {
    let param_name = &param.name.to_string();
    let param_ident = &param.name;
    let param_type = &param.ty;

    let missing_err = error_handling::generate_missing_param_error(param_name, "mcp");
    let parse_err =
        error_handling::generate_parse_error(param_name, &quote!(#param_type).to_string(), "mcp");

    if is_optional_type(&param.ty) {
        quote! {
            let #param_ident: #param_type = params.get(#param_name)
                .map(|v| ::serde_json::from_value(v.clone())
                    .map_err(|_| #parse_err))
                .transpose()?;
        }
    } else {
        quote! {
            let #param_ident: #param_type = params.get(#param_name)
                .ok_or_else(|| #missing_err)
                .and_then(|v| ::serde_json::from_value(v.clone())
                    .map_err(|_| #parse_err))?;
        }
    }
}

/// Generates extraction code for all parameters of a tool
pub fn generate_params_extraction(tool: &ToolDef, interface: &str) -> Vec<TokenStream> {
    tool.params
        .iter()
        .filter(|p| should_include_param(p, interface))
        .map(|param| generate_param_extraction(param, interface))
        .collect()
}

/// Checks if a parameter should be included based on interface
pub fn should_include_param(param: &ParamDef, interface: &str) -> bool {
    match interface {
        "mcp" => {
            // MCP skips ProgressReporter and CancellationToken
            let param_ty = &param.ty;
            let type_str = quote!(#param_ty).to_string();
            !type_str.contains("ProgressReporter") && !type_str.contains("CancellationToken")
        }
        _ => true,
    }
}

/// Extracts the inner type from Option<T>
fn extract_option_inner_type(ty: &Type) -> Type {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.first() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return inner_ty.clone();
                    }
                }
            }
        }
    }
    // Fallback to String if we can't extract the type
    parse_str("String").unwrap()
}

/// Extracts the value type from HashMap<K, V>
fn extract_hashmap_value_type(ty: &Type) -> Type {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "HashMap" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    // HashMap has two type arguments, we want the second one (value type)
                    if let Some(syn::GenericArgument::Type(value_ty)) = args.args.iter().nth(1) {
                        return value_ty.clone();
                    }
                }
            }
        }
    }
    // Fallback to String if we can't extract the type
    parse_str("String").unwrap()
}

/// Generates the appropriate value parser for CLI parameters
pub fn generate_cli_value_parser(param: &ParamDef) -> Option<TokenStream> {
    let param_type = &param.ty;

    if is_bool_type(&param.ty)
        || is_vec_type(&param.ty)
        || is_custom_struct_type(&param.ty)
        || is_hashmap_type(&param.ty)
    {
        // These types don't use value_parser
        None
    } else if is_optional_type(&param.ty) {
        // For Option<T>, we need to extract the inner type for the value parser
        let inner_type = extract_option_inner_type(&param.ty);
        Some(quote! { ::universal_tool_core::cli::clap::value_parser!(#inner_type) })
    } else {
        Some(quote! { ::universal_tool_core::cli::clap::value_parser!(#param_type) })
    }
}

/// Generates the argument configuration for CLI parameters
pub fn generate_cli_arg_config(param: &ParamDef) -> TokenStream {
    let param_name = &param.name.to_string();
    let description = param
        .metadata
        .description
        .as_deref()
        .unwrap_or("Parameter value");

    // Determine all the method calls we need
    let value_parser = generate_cli_value_parser(param);
    let is_required = !is_optional_type(&param.ty)
        && !is_bool_type(&param.ty)
        && !is_vec_type(&param.ty)
        && !is_hashmap_type(&param.ty);
    let is_bool = is_bool_type(&param.ty);
    let is_multi = is_vec_type(&param.ty) || is_hashmap_type(&param.ty);

    // Build the base arg configuration
    let base_arg = match (value_parser, is_required, is_bool, is_multi) {
        (Some(vp), true, false, false) => quote! {
            ::universal_tool_core::cli::clap::Arg::new(#param_name)
                .long(#param_name)
                .help(#description)
                .value_parser(#vp)
                .required(true)
        },
        (Some(vp), false, false, false) => quote! {
            ::universal_tool_core::cli::clap::Arg::new(#param_name)
                .long(#param_name)
                .help(#description)
                .value_parser(#vp)
        },
        (None, true, false, false) => quote! {
            ::universal_tool_core::cli::clap::Arg::new(#param_name)
                .long(#param_name)
                .help(#description)
                .required(true)
        },
        (None, false, true, false) => quote! {
            ::universal_tool_core::cli::clap::Arg::new(#param_name)
                .long(#param_name)
                .help(#description)
                .action(::universal_tool_core::cli::clap::ArgAction::SetTrue)
        },
        (None, false, false, true) => quote! {
            ::universal_tool_core::cli::clap::Arg::new(#param_name)
                .long(#param_name)
                .help(#description)
                .action(::universal_tool_core::cli::clap::ArgAction::Append)
        },
        _ => quote! {
            ::universal_tool_core::cli::clap::Arg::new(#param_name)
                .long(#param_name)
                .help(#description)
        },
    };

    // Add environment variable support if specified
    let with_env = if let Some(env_var) = &param.metadata.env {
        quote! { .env(#env_var) }
    } else {
        quote! {}
    };

    // Add default value support if specified
    let with_default = if let Some(default_val) = &param.metadata.default {
        quote! { .default_value(#default_val) }
    } else {
        quote! {}
    };

    // Combine all the pieces
    quote! {
        #base_arg
            #with_env
            #with_default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_should_include_param() {
        let normal_param = ParamDef {
            name: parse_quote!(input),
            ty: parse_quote!(String),
            source: crate::model::ParamSource::Body,
            is_optional: false,
            metadata: Default::default(),
        };

        assert!(should_include_param(&normal_param, "cli"));
        assert!(should_include_param(&normal_param, "rest"));
        assert!(should_include_param(&normal_param, "mcp"));

        let progress_param = ParamDef {
            name: parse_quote!(progress),
            ty: parse_quote!(ProgressReporter),
            source: crate::model::ParamSource::Body,
            is_optional: false,
            metadata: Default::default(),
        };

        assert!(should_include_param(&progress_param, "cli"));
        assert!(should_include_param(&progress_param, "rest"));
        assert!(!should_include_param(&progress_param, "mcp"));
    }
}
