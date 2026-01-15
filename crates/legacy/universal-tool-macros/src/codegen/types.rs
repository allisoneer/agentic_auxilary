//! Type analysis utilities for code generation
//!
//! Provides utilities for analyzing Rust types to generate appropriate
//! code for different interfaces (CLI, REST, MCP).

#![allow(dead_code)]

use syn::{GenericArgument, PathArguments, Type};

/// Check if a type is Option<T>
pub fn is_option(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return segment.ident == "Option";
    }
    false
}

/// Check if a type is Vec<T> or similar collection
pub fn is_vec(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return segment.ident == "Vec";
    }
    false
}

/// Extract the inner type from Option<T>, Vec<T>, etc.
/// Returns the inner type if it's a generic type with one argument,
/// otherwise returns the original type.
pub fn extract_inner_type(ty: &Type) -> &Type {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && let PathArguments::AngleBracketed(args) = &segment.arguments
        && args.args.len() == 1
        && let GenericArgument::Type(inner_ty) = &args.args[0]
    {
        return inner_ty;
    }
    ty
}

/// Get the last segment name of a type path (e.g., "String" from "std::string::String")
pub fn get_type_name(ty: &Type) -> Option<String> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
    {
        return Some(segment.ident.to_string());
    }
    None
}

/// Check if a type is a primitive type that doesn't need explicit deserialization
pub fn is_primitive_type(ty: &Type) -> bool {
    if let Some(name) = get_type_name(ty) {
        matches!(
            name.as_str(),
            "bool"
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
                | "String"
                | "str"
        )
    } else {
        false
    }
}

/// Check if a type path matches a specific type name
pub fn is_type_path(ty: &Type, expected: &str) -> bool {
    if let Type::Path(type_path) = ty {
        // Handle both simple names and qualified paths
        let path_str = quote::quote!(#type_path).to_string();
        path_str == expected || path_str.ends_with(&format!("::{expected}"))
    } else {
        false
    }
}

/// Extract generic arguments from a type
pub fn get_generic_args(ty: &Type) -> Vec<&Type> {
    if let Type::Path(type_path) = ty
        && let Some(segment) = type_path.path.segments.last()
        && let PathArguments::AngleBracketed(args) = &segment.arguments
    {
        return args
            .args
            .iter()
            .filter_map(|arg| {
                if let GenericArgument::Type(ty) = arg {
                    Some(ty)
                } else {
                    None
                }
            })
            .collect();
    }
    vec![]
}

/// Check if a type is a HashMap or similar map type
pub fn is_map_type(ty: &Type) -> bool {
    if let Some(name) = get_type_name(ty) {
        matches!(name.as_str(), "HashMap" | "BTreeMap")
    } else {
        false
    }
}

/// Format a type for use in generated code (removes lifetime parameters)
pub fn format_type_for_codegen(ty: &Type) -> String {
    // TODO(2): This is a simplified version. In a real implementation,
    // we'd want to properly handle lifetimes and complex generic types.
    quote::quote!(#ty).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_is_option() {
        let opt_ty: Type = parse_quote!(Option<String>);
        let non_opt_ty: Type = parse_quote!(String);

        assert!(is_option(&opt_ty));
        assert!(!is_option(&non_opt_ty));
    }

    #[test]
    fn test_extract_inner_type() {
        let opt_ty: Type = parse_quote!(Option<String>);
        let inner = extract_inner_type(&opt_ty);
        assert_eq!(quote::quote!(#inner).to_string(), "String");

        let vec_ty: Type = parse_quote!(Vec<i32>);
        let inner = extract_inner_type(&vec_ty);
        assert_eq!(quote::quote!(#inner).to_string(), "i32");
    }

    #[test]
    fn test_is_primitive_type() {
        let string_ty: Type = parse_quote!(String);
        let custom_ty: Type = parse_quote!(MyCustomType);

        assert!(is_primitive_type(&string_ty));
        assert!(!is_primitive_type(&custom_ty));
    }
}
