//! Parameter struct generation utilities
//!
//! Generates structs for deserializing parameters across all interfaces.

#![allow(dead_code)]

use crate::model::{ParamDef, ToolDef};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Generate a parameter struct for a tool
///
/// This generates a struct with all the parameters of a tool method,
/// suitable for deserialization from JSON, CLI args, etc.
pub fn generate_param_struct(tool: &ToolDef, struct_name: &str, derives: &[&str]) -> TokenStream {
    let struct_ident = format_ident!("{}", struct_name);

    // Filter out self parameter
    let params: Vec<&ParamDef> = tool.params.iter().filter(|p| p.name != "self").collect();

    if params.is_empty() {
        // Empty struct for tools with no parameters
        let derive_tokens = generate_derives(derives);
        return quote! {
            #derive_tokens
            pub struct #struct_ident {}
        };
    }

    // Generate fields
    let fields = params.iter().map(|param| {
        let field_name = &param.name;
        let field_type = &param.ty;

        // Add documentation if available
        let docs = if let Some(desc) = &param.metadata.description {
            quote! { #[doc = #desc] }
        } else {
            quote! {}
        };

        // Add serde attributes if needed
        let serde_attrs = generate_serde_attrs(param);

        quote! {
            #docs
            #serde_attrs
            pub #field_name: #field_type
        }
    });

    let derive_tokens = generate_derives(derives);

    quote! {
        #derive_tokens
        pub struct #struct_ident {
            #(#fields),*
        }
    }
}

/// Generate inline parameter struct (for use inside functions)
pub fn generate_inline_param_struct(
    tool: &ToolDef,
    struct_name: &str,
    derives: &[&str],
) -> TokenStream {
    let struct_ident = format_ident!("{}", struct_name);

    // Filter out self parameter
    let params: Vec<&ParamDef> = tool.params.iter().filter(|p| p.name != "self").collect();

    if params.is_empty() {
        let derive_tokens = generate_derives(derives);
        return quote! {
            #derive_tokens
            struct #struct_ident {}
        };
    }

    let fields = params.iter().map(|param| {
        let field_name = &param.name;
        let field_type = &param.ty;
        let serde_attrs = generate_serde_attrs(param);

        quote! {
            #serde_attrs
            #field_name: #field_type
        }
    });

    let derive_tokens = generate_derives(derives);

    quote! {
        #derive_tokens
        struct #struct_ident {
            #(#fields),*
        }
    }
}

/// Generate derive macros from string names
fn generate_derives(derives: &[&str]) -> TokenStream {
    let derive_idents: Vec<TokenStream> = derives
        .iter()
        .map(|d| {
            // Handle common derive macros with proper paths
            match *d {
                "Serialize" => quote!(serde::Serialize),
                "Deserialize" => quote!(serde::Deserialize),
                "JsonSchema" => quote!(schemars::JsonSchema),
                "ToSchema" => quote!(utoipa::ToSchema),
                "Debug" => quote!(Debug),
                "Clone" => quote!(Clone),
                _ => {
                    let ident = format_ident!("{}", d);
                    quote!(#ident)
                }
            }
        })
        .collect();

    if derive_idents.is_empty() {
        quote! {}
    } else {
        quote! {
            #[derive(#(#derive_idents),*)]
        }
    }
}

/// Generate serde attributes for a parameter
fn generate_serde_attrs(param: &ParamDef) -> TokenStream {
    let mut attrs = vec![];

    // Add rename if field name is a Rust keyword
    if is_rust_keyword(&param.name.to_string()) {
        let renamed = format!("{}_", param.name);
        attrs.push(quote! { #[serde(rename = #renamed)] });
    }

    // Add default for optional parameters
    if param.is_optional {
        attrs.push(quote! { #[serde(default)] });
    }

    // Combine all attributes
    quote! { #(#attrs)* }
}

/// Check if a name is a Rust keyword that needs renaming
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

/// Generate a method call with parameters from a struct instance
pub fn generate_method_call_from_struct(tool: &ToolDef, struct_var: &str) -> TokenStream {
    let method_name = &tool.method_name;

    // Get non-self parameters
    let params: Vec<&ParamDef> = tool.params.iter().filter(|p| p.name != "self").collect();

    // Generate parameter list
    let args = params.iter().map(|param| {
        let field_name = &param.name;
        let struct_ident = format_ident!("{}", struct_var);
        quote! { #struct_ident.#field_name }
    });

    // Handle async methods
    if tool.is_async {
        quote! {
            self.#method_name(#(#args),*).await
        }
    } else {
        quote! {
            self.#method_name(#(#args),*)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ParamMetadata, ParamSource};
    use syn::{Type, parse_quote};

    fn create_test_param(name: &str, ty: Type, is_optional: bool) -> ParamDef {
        ParamDef {
            name: format_ident!("{}", name),
            ty,
            source: ParamSource::Body,
            is_optional,
            metadata: ParamMetadata {
                description: Some(format!("Test parameter {name}")),
                ..Default::default()
            },
        }
    }

    #[test]
    fn test_generate_param_struct() {
        let tool = ToolDef {
            method_name: format_ident!("test_method"),
            tool_name: "test".to_string(),
            params: vec![
                create_test_param("name", parse_quote!(String), false),
                create_test_param("count", parse_quote!(Option<u32>), true),
            ],
            return_type: parse_quote!(String),
            is_async: false,
            visibility: parse_quote!(pub),
            metadata: Default::default(),
        };

        let tokens = generate_param_struct(&tool, "TestParams", &["Deserialize", "JsonSchema"]);
        let generated = tokens.to_string();

        assert!(generated.contains("struct TestParams"));
        assert!(generated.contains("pub name : String"));
        assert!(generated.contains("pub count : Option < u32 >"));
        assert!(generated.contains("serde :: Deserialize"));
        assert!(generated.contains("schemars :: JsonSchema"));
    }

    #[test]
    fn test_empty_param_struct() {
        let tool = ToolDef {
            method_name: format_ident!("test_method"),
            tool_name: "test".to_string(),
            params: vec![],
            return_type: parse_quote!(()),
            is_async: false,
            visibility: parse_quote!(pub),
            metadata: Default::default(),
        };

        let tokens = generate_param_struct(&tool, "EmptyParams", &["Deserialize"]);
        let generated = tokens.to_string();

        assert!(generated.contains("struct EmptyParams { }"));
    }
}
