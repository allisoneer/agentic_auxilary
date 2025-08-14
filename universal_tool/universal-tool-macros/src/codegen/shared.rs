//! Shared code generation utilities
//!
//! Common utilities used by all interface generators (CLI, REST, MCP).

#![allow(dead_code)]

use crate::model::ToolDef;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Type;

// TODO(1): Move these interface-specific functions to their respective modules (cli.rs, rest.rs, mcp.rs)
// For now, keeping them here to support early testing and development

/// Generate error conversion from ToolError to interface-specific error
pub fn generate_error_conversion(error_var: &str, interface: &str) -> TokenStream {
    let err = format_ident!("{}", error_var);

    match interface {
        "cli" => quote! {
            eprintln!("Error: {}", #err);
            std::process::exit(1);
        },
        "rest" => quote! {
            let status = match #err.code() {
                universal_tool::ErrorCode::InvalidInput => axum::http::StatusCode::BAD_REQUEST,
                universal_tool::ErrorCode::NotFound => axum::http::StatusCode::NOT_FOUND,
                universal_tool::ErrorCode::PermissionDenied => axum::http::StatusCode::FORBIDDEN,
                universal_tool::ErrorCode::Conflict => axum::http::StatusCode::CONFLICT,
                universal_tool::ErrorCode::Internal => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            (status, axum::Json(serde_json::json!({
                "error": #err.to_string(),
                "code": #err.code().to_string(),
            }))).into_response()
        },
        "mcp" => quote! {
            Err(mcp_sdk::Error::Custom {
                code: match #err.code() {
                    universal_tool::ErrorCode::InvalidInput => -32602,
                    universal_tool::ErrorCode::NotFound => -32601,
                    universal_tool::ErrorCode::PermissionDenied => -32603,
                    universal_tool::ErrorCode::Conflict => -32604,
                    universal_tool::ErrorCode::Internal => -32603,
                },
                message: #err.to_string(),
                data: None,
            })
        },
        _ => quote! { Err(#err) },
    }
}

/// Generate async handling wrapper
///
/// Wraps async method calls appropriately for each interface:
/// - CLI: Uses block_on
/// - REST/MCP: Natural async support
pub fn generate_async_wrapper(inner: TokenStream, is_async: bool, interface: &str) -> TokenStream {
    if !is_async {
        return inner;
    }

    match interface {
        "cli" => quote! {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async { #inner })
        },
        _ => inner,
    }
}

/// Generate a method call that works for both async and sync methods
/// This handles the normalization between sync and async methods
pub fn generate_normalized_method_call(
    tool: &ToolDef,
    self_expr: TokenStream,
    args: Vec<TokenStream>,
) -> TokenStream {
    let method_name = &tool.method_name;
    let method_call = quote! {
        #self_expr.#method_name(#(#args),*)
    };

    if tool.is_async {
        quote! { #method_call.await }
    } else {
        // Sync method - no await needed
        method_call
    }
}

/// Generate method signature for documentation/schemas
pub fn generate_method_signature(tool: &ToolDef) -> String {
    let params = tool
        .params
        .iter()
        .filter(|p| p.name != "self")
        .map(|p| {
            let ty = &p.ty;
            format!("{}: {}", p.name, quote!(#ty))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let return_type = &tool.return_type;
    let return_type_str = quote!(#return_type).to_string();

    format!(
        "{}fn {}({}) -> Result<{}, ToolError>",
        if tool.is_async { "async " } else { "" },
        tool.method_name,
        params,
        return_type_str
    )
}

/// Extract description from tool metadata or doc comments
pub fn get_tool_description(tool: &ToolDef) -> String {
    if tool.metadata.description.is_empty() {
        format!("Execute {} operation", tool.tool_name)
    } else {
        tool.metadata.description.clone()
    }
}

/// Generate validation for required parameters
pub fn generate_param_validation(tool: &ToolDef) -> TokenStream {
    let validations = tool
        .params
        .iter()
        .filter(|p| p.name != "self" && !p.is_optional)
        .map(|param| {
            let _param_name = &param.name;

            // For non-optional parameters that aren't already Option<T>
            quote! {
                // Validation is handled by type system for required params
            }
        });

    quote! { #(#validations)* }
}

/// Generate JSON schema for a type
pub fn generate_json_schema(ty: &Type) -> TokenStream {
    // This is a simplified version - real implementation would handle
    // complex types more thoroughly
    quote! {
        <#ty as schemars::JsonSchema>::json_schema(gen)
    }
}

/// Check if a method should be exposed on a given interface
pub fn should_expose_on_interface(tool: &ToolDef, interface: &str) -> bool {
    match interface {
        "cli" => tool
            .metadata
            .cli_config
            .as_ref()
            .is_none_or(|c| !c.hidden),
        "rest" => tool.metadata.rest_config.is_some(),
        "mcp" => tool.metadata.mcp_config.is_some(),
        _ => true,
    }
}

/// Generate parameter documentation
pub fn generate_param_docs(tool: &ToolDef) -> Vec<String> {
    tool.params
        .iter()
        .filter(|p| p.name != "self")
        .map(|param| {
            let required = if param.is_optional {
                "optional"
            } else {
                "required"
            };
            let desc = param
                .metadata
                .description
                .as_deref()
                .unwrap_or("No description");
            format!("- {} ({}) - {}", param.name, required, desc)
        })
        .collect()
}

/// Generate progress notification helper (for long-running operations)
pub fn generate_progress_helper(_tool_name: &str, _message: &str) -> TokenStream {
    // TODO(3): Add progress feature support when implemented
    // For now, always generate empty code since progress feature doesn't exist yet
    quote! {}
}

/// Check if we need to generate special handling for streaming
/// (UTF doesn't support streaming, but we need to detect and error on it)
pub fn check_streaming_type(return_type: &Type) -> bool {
    let type_str = quote!(#return_type).to_string();
    type_str.contains("Stream") || type_str.contains("Streaming")
}

/// Convert any case format to kebab-case
/// Handles: camelCase, PascalCase, snake_case, kebab-case, and mixed formats
pub fn to_kebab_case(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' {
            if !result.is_empty() && !result.ends_with('-') {
                result.push('-');
            }
        } else if ch.is_uppercase() {
            // Check if we need to insert a hyphen
            if i > 0 && !result.ends_with('-') {
                let prev = chars[i - 1];
                let next = chars.get(i + 1);

                // Insert hyphen if:
                // 1. Previous char was lowercase, or
                // 2. Previous char was uppercase and next char is lowercase (end of acronym)
                if prev.is_lowercase()
                    || (prev.is_uppercase() && next.is_some_and(|&c| c.is_lowercase()))
                {
                    result.push('-');
                }
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }

    result
}

/// Convert any case format to snake_case
/// Handles: camelCase, PascalCase, snake_case, kebab-case, and mixed formats
pub fn to_snake_case(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch == '-' || ch == '_' {
            if !result.is_empty() && !result.ends_with('_') {
                result.push('_');
            }
        } else if ch.is_uppercase() {
            // Check if we need to insert an underscore
            if i > 0 && !result.ends_with('_') {
                let prev = chars[i - 1];
                let next = chars.get(i + 1);

                // Insert underscore if:
                // 1. Previous char was lowercase, or
                // 2. Previous char was uppercase and next char is lowercase (end of acronym)
                if prev.is_lowercase()
                    || (prev.is_uppercase() && next.is_some_and(|&c| c.is_lowercase()))
                {
                    result.push('_');
                }
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }

    result
}

/// Convert any case format to camelCase
/// Handles: camelCase, PascalCase, snake_case, kebab-case, and mixed formats
pub fn to_camel_case(input: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    let mut is_first = true;

    for ch in input.chars() {
        if ch == '_' || ch == '-' {
            capitalize_next = true;
        } else if capitalize_next || (is_first && ch.is_uppercase()) {
            // First character should be lowercase in camelCase
            if is_first {
                result.push(ch.to_lowercase().next().unwrap());
                is_first = false;
            } else {
                result.push(ch.to_uppercase().next().unwrap());
            }
            capitalize_next = false;
        } else {
            result.push(ch);
            is_first = false;
        }
    }

    result
}

/// Convert any case format to PascalCase
/// Handles: camelCase, PascalCase, snake_case, kebab-case, and mixed formats  
pub fn to_pascal_case(input: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for ch in input.chars() {
        if ch == '_' || ch == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }

    result
}

// Deprecated: Use the more general to_kebab_case function instead
#[deprecated(note = "Use to_kebab_case instead")]
pub fn snake_to_kebab_case(input: &str) -> String {
    to_kebab_case(input)
}

// Deprecated: Use the more general to_camel_case function instead
#[deprecated(note = "Use to_camel_case instead")]
pub fn snake_to_camel_case(input: &str) -> String {
    to_camel_case(input)
}

// Deprecated: Use the more general to_pascal_case function instead
#[deprecated(note = "Use to_pascal_case instead")]
pub fn snake_to_pascal_case(input: &str) -> String {
    to_pascal_case(input)
}

/// Sanitize a name for use as a Rust identifier
pub fn sanitize_identifier(name: &str) -> String {
    // Handle reserved keywords by appending underscore
    if is_rust_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// Check if a name is a Rust keyword
fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "dyn"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
            | "try"
    )
}

// Type checking helper functions

/// Converts a Type to a string representation
pub fn type_to_string(ty: &Type) -> String {
    quote!(#ty).to_string().replace(' ', "")
}

/// Checks if a type is bool
pub fn is_bool_type(ty: &Type) -> bool {
    matches!(type_to_string(ty).as_str(), "bool")
}

/// Checks if a type is String
pub fn is_string_type(ty: &Type) -> bool {
    matches!(type_to_string(ty).as_str(), "String")
}

/// Checks if a type is Vec<T>
pub fn is_vec_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.first() {
            return segment.ident == "Vec";
        }
    }
    false
}

/// Checks if a type is HashMap<K, V>
pub fn is_hashmap_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "HashMap";
        }
    }
    false
}

/// Checks if a type is Option<T>
pub fn is_optional_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.first() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Checks if a type is a custom struct (not a primitive, String, Vec, HashMap, etc.)
pub fn is_custom_struct_type(ty: &Type) -> bool {
    let ty_str = type_to_string(ty);
    // If it's not a known type, assume it's a custom struct
    !matches!(
        ty_str.as_str(),
        "String"
            | "str"
            | "bool"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "char"
    ) && !is_vec_type(ty)
        && !is_hashmap_type(ty)
        && !ty_str.starts_with("Option<")
}

/// Checks if a type is a numeric type
pub fn is_numeric_type(ty: &Type) -> bool {
    matches!(
        type_to_string(ty).as_str(),
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_generate_method_signature() {
        let tool = ToolDef {
            method_name: format_ident!("analyze"),
            tool_name: "analyze".to_string(),
            params: vec![
                crate::model::ParamDef {
                    name: format_ident!("self"),
                    ty: parse_quote!(&self),
                    source: crate::model::ParamSource::Body,
                    is_optional: false,
                    metadata: Default::default(),
                },
                crate::model::ParamDef {
                    name: format_ident!("path"),
                    ty: parse_quote!(String),
                    source: crate::model::ParamSource::Body,
                    is_optional: false,
                    metadata: Default::default(),
                },
            ],
            return_type: parse_quote!(AnalysisResult),
            is_async: true,
            metadata: Default::default(),
            visibility: parse_quote!(pub),
        };

        let sig = generate_method_signature(&tool);
        assert_eq!(
            sig,
            "async fn analyze(path: String) -> Result<AnalysisResult, ToolError>"
        );
    }

    #[test]
    fn test_error_conversion() {
        let cli_error = generate_error_conversion("err", "cli");
        assert!(cli_error.to_string().contains("eprintln"));

        let rest_error = generate_error_conversion("err", "rest");
        assert!(rest_error.to_string().contains("StatusCode"));
    }

    #[test]
    fn test_name_conversions() {
        // Test to_kebab_case with various inputs
        assert_eq!(to_kebab_case("hello_world"), "hello-world");
        assert_eq!(to_kebab_case("helloWorld"), "hello-world");
        assert_eq!(to_kebab_case("HelloWorld"), "hello-world");
        assert_eq!(to_kebab_case("hello-world"), "hello-world");
        assert_eq!(to_kebab_case("simple"), "simple");
        assert_eq!(to_kebab_case("HTTPSConnection"), "https-connection");
        assert_eq!(to_kebab_case("getHTTPResponse"), "get-http-response");

        // Test to_snake_case with various inputs
        assert_eq!(to_snake_case("hello-world"), "hello_world");
        assert_eq!(to_snake_case("helloWorld"), "hello_world");
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("hello_world"), "hello_world");
        assert_eq!(to_snake_case("HTTPSConnection"), "https_connection");

        // Test to_camel_case with various inputs
        assert_eq!(to_camel_case("hello_world"), "helloWorld");
        assert_eq!(to_camel_case("hello-world"), "helloWorld");
        assert_eq!(to_camel_case("HelloWorld"), "helloWorld");
        assert_eq!(to_camel_case("helloWorld"), "helloWorld");
        assert_eq!(to_camel_case("simple"), "simple");
        assert_eq!(to_camel_case("multi_word_name"), "multiWordName");

        // Test to_pascal_case with various inputs
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("hello-world"), "HelloWorld");
        assert_eq!(to_pascal_case("helloWorld"), "HelloWorld");
        assert_eq!(to_pascal_case("HelloWorld"), "HelloWorld");
        assert_eq!(to_pascal_case("simple"), "Simple");
        assert_eq!(to_pascal_case("multi_word_name"), "MultiWordName");
    }

    #[test]
    fn test_sanitize_identifier() {
        assert_eq!(sanitize_identifier("normal_name"), "normal_name");
        assert_eq!(sanitize_identifier("type"), "type_");
        assert_eq!(sanitize_identifier("async"), "async_");
        assert_eq!(sanitize_identifier("match"), "match_");
    }
}
