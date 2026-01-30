//! Implementation of the #[derive(TextFormat)] proc macro.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Expr, Lit, parse2};

/// Expand the #[derive(TextFormat)] macro.
///
/// Supports an optional `#[text_format(with = "path::to_fn")]` attribute to delegate
/// formatting to a custom function.
///
/// # Example
///
/// ```ignore
/// #[derive(TextFormat)]
/// struct MyOutput {
///     message: String,
/// }
///
/// // Or with custom formatter:
/// #[derive(TextFormat)]
/// #[text_format(with = "my_module::format_my_output")]
/// struct MyOutput {
///     message: String,
/// }
/// ```
pub fn expand(input: TokenStream) -> syn::Result<TokenStream> {
    let input: DeriveInput = parse2(input)?;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Look for #[text_format(with = "path::to_fn")] attribute
    let mut with_fn: Option<syn::Path> = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("text_format") {
            continue;
        }
        // Parse the attribute as a nested meta list
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("with") {
                let value: Expr = meta.value()?.parse()?;
                if let Expr::Lit(expr_lit) = value
                    && let Lit::Str(lit_str) = expr_lit.lit
                {
                    with_fn = Some(lit_str.parse()?);
                }
            } else {
                return Err(meta.error("unknown text_format attribute"));
            }
            Ok(())
        })?;
    }

    let impl_block = if let Some(path) = with_fn {
        // Custom formatter function provided
        quote! {
            impl #impl_generics agentic_tools_core::fmt::TextFormat for #name #ty_generics #where_clause {
                fn fmt_text(&self, opts: &agentic_tools_core::fmt::TextOptions) -> String {
                    #path(self, opts)
                }
            }
        }
    } else {
        // Default implementation: pretty JSON, optionally wrapped in markdown
        quote! {
            impl #impl_generics agentic_tools_core::fmt::TextFormat for #name #ty_generics #where_clause {
                fn fmt_text(&self, opts: &agentic_tools_core::fmt::TextOptions) -> String {
                    let json = serde_json::to_string_pretty(self)
                        .unwrap_or_else(|_| "<serialization error>".to_string());
                    if opts.markdown {
                        format!("```json\n{}\n```", json)
                    } else {
                        json
                    }
                }
            }
        }
    };

    Ok(impl_block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_derive_without_attributes() {
        let input = quote! {
            struct TestOutput {
                value: String,
            }
        };
        let result = expand(input);
        assert!(result.is_ok(), "Derive without attributes should succeed");

        let tokens = result.unwrap().to_string();
        assert!(tokens.contains("impl"));
        assert!(tokens.contains("TextFormat"));
        // Quote produces space-separated paths
        assert!(tokens.contains("serde_json :: to_string_pretty"));
    }

    #[test]
    fn test_derive_with_custom_function() {
        let input = quote! {
            #[text_format(with = "my_module::format_fn")]
            struct TestOutput {
                value: String,
            }
        };
        let result = expand(input);
        assert!(result.is_ok(), "Derive with custom function should succeed");

        let tokens = result.unwrap().to_string();
        assert!(tokens.contains("impl"));
        assert!(tokens.contains("TextFormat"));
        assert!(tokens.contains("my_module :: format_fn"));
    }

    #[test]
    fn test_derive_with_generics() {
        let input = quote! {
            struct TestOutput<T> {
                value: T,
            }
        };
        let result = expand(input);
        assert!(result.is_ok(), "Derive with generics should succeed");

        let tokens = result.unwrap().to_string();
        assert!(tokens.contains("impl < T >"));
    }
}
