//! Implementation of the #[tool] proc macro.

use darling::FromMeta;
use darling::ast::NestedMeta;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, ReturnType, Type, parse2};

/// Parsed #[tool(...)] attributes.
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct ToolAttr {
    /// Override the tool name (defaults to function name).
    name: Option<String>,
    /// Description of what the tool does.
    description: Option<String>,
}

/// Expand the #[tool] attribute macro.
pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let func: ItemFn = parse2(item)?;

    // Validate function is async (C1)
    if func.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            func.sig.fn_token,
            "tool function must be async",
        ));
    }

    // Parse attributes using darling (M3)
    let tool_attr = if attr.is_empty() {
        ToolAttr::default()
    } else {
        let nested = NestedMeta::parse_meta_list(attr.clone())
            .map_err(|e| syn::Error::new_spanned(&attr, e))?;
        ToolAttr::from_list(&nested).map_err(|e| syn::Error::new_spanned(&attr, e))?
    };

    let fn_ident = &func.sig.ident;
    let tool_name = tool_attr.name.unwrap_or_else(|| fn_ident.to_string());
    let tool_description = tool_attr.description.unwrap_or_default();

    // Create PascalCase struct name from function name
    let struct_name = to_pascal_case(&fn_ident.to_string());
    let tool_struct = format_ident!("{}Tool", struct_name);

    // Validate parameters
    let inputs = &func.sig.inputs;

    // Reject receivers
    if inputs.iter().any(|arg| matches!(arg, FnArg::Receiver(_))) {
        return Err(syn::Error::new_spanned(
            inputs,
            "#[tool] must be applied to a free function, not a method with a receiver (`self`). \
             Pass state via the input struct instead.",
        ));
    }

    // Collect typed parameters
    let typed_params: Vec<&syn::PatType> = inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pat_type) => Some(pat_type),
            _ => None,
        })
        .collect();

    // Validate arity and ctx type
    let (input_type, forwards_ctx) = match typed_params.as_slice() {
        [] => {
            return Err(syn::Error::new_spanned(
                inputs,
                "#[tool] functions must take (input: T) or (input: T, ctx: &ToolContext). \
                 If you need multiple inputs, wrap them in a single input struct.",
            ));
        }
        [input] => ((*input.ty).clone(), false),
        [input, ctx] => {
            if !is_tool_context_ref(&ctx.ty) {
                return Err(syn::Error::new_spanned(
                    &ctx.ty,
                    "second parameter of a #[tool] function must be `ctx: &ToolContext`. \
                     Other extra parameters are not supported; put additional fields \
                     into the input type instead.",
                ));
            }
            ((*input.ty).clone(), true)
        }
        _ => {
            return Err(syn::Error::new_spanned(
                inputs,
                "#[tool] functions must take (input: T) or (input: T, ctx: &ToolContext). \
                 If you need multiple inputs, wrap them in a single input struct.",
            ));
        }
    };

    // Extract output type from return type
    // Assume Result<Output, ToolError> and extract Output
    let output_type: Type = match &func.sig.output {
        ReturnType::Type(_, ty) => extract_result_ok_type(ty).unwrap_or_else(|| (**ty).clone()),
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &func.sig,
                "tool function must have a return type",
            ));
        }
    };

    let ctx_param = if forwards_ctx {
        quote!(ctx: &agentic_tools_core::ToolContext)
    } else {
        quote!(_ctx: &agentic_tools_core::ToolContext)
    };

    let call_body = if forwards_ctx {
        quote! {
            let ctx = ctx.clone();
            Box::pin(async move { #fn_ident(input, &ctx).await })
        }
    } else {
        quote! {
            Box::pin(#fn_ident(input))
        }
    };

    let doc_comment = format!("Auto-generated tool struct for [`{}`].", fn_ident);

    let expanded = quote! {
        #func

        #[doc = #doc_comment]
        #[derive(Clone)]
        pub struct #tool_struct;

        impl agentic_tools_core::Tool for #tool_struct {
            type Input = #input_type;
            type Output = #output_type;
            const NAME: &'static str = #tool_name;
            const DESCRIPTION: &'static str = #tool_description;

            fn call(
                &self,
                input: Self::Input,
                #ctx_param,
            ) -> agentic_tools_core::BoxFuture<'static, Result<Self::Output, agentic_tools_core::ToolError>>
            {
                #call_body
            }
        }
    };

    Ok(expanded)
}

/// Convert snake_case to PascalCase.
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

/// Try to extract the Ok type from a Result<T, E> type.
fn extract_result_ok_type(ty: &Type) -> Option<Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let last_segment = type_path.path.segments.last()?;
    if last_segment.ident == "Result"
        && let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments
        && let Some(syn::GenericArgument::Type(ok_type)) = args.args.first()
    {
        return Some(ok_type.clone());
    }
    None
}

/// Check whether a type is `&ToolContext` (immutable reference, last segment ident match).
fn is_tool_context_ref(ty: &Type) -> bool {
    let Type::Reference(ty_ref) = ty else {
        return false;
    };
    if ty_ref.mutability.is_some() {
        return false;
    }
    let Type::Path(type_path) = ty_ref.elem.as_ref() else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "ToolContext")
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("get_comments"), "GetComments");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_expand_rejects_non_async() {
        let attr = quote!(name = "greet");
        let item = quote! {
            fn greet(input: String) -> Result<String, agentic_tools_core::ToolError> {
                Ok(input)
            }
        };
        let res = expand(attr, item);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("tool function must be async"),
            "Expected error about async, got: {msg}"
        );
    }

    #[test]
    fn test_darling_parses_description_with_commas() {
        let attr = quote!(description = "Fetch X, Y, and Z");
        let item = quote! {
            async fn fetch(input: String) -> Result<String, agentic_tools_core::ToolError> {
                Ok(input)
            }
        };
        let res = expand(attr, item);
        assert!(
            res.is_ok(),
            "darling should parse commas within quoted description: {:?}",
            res.unwrap_err()
        );
    }

    #[test]
    fn test_expand_rejects_receiver() {
        let item = quote! {
            async fn greet(&self, input: String) -> Result<String, agentic_tools_core::ToolError> {
                Ok(input)
            }
        };
        let res = expand(quote!(), item);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("self"),
            "Expected error about receiver, got: {msg}"
        );
    }

    #[test]
    fn test_expand_rejects_zero_params() {
        let item = quote! {
            async fn greet() -> Result<String, agentic_tools_core::ToolError> {
                Ok("hi".to_string())
            }
        };
        let res = expand(quote!(), item);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("(input: T)"),
            "Expected guidance about parameters, got: {msg}"
        );
    }

    #[test]
    fn test_expand_rejects_three_params() {
        let item = quote! {
            async fn greet(
                input: String,
                ctx: &agentic_tools_core::ToolContext,
                extra: u8,
            ) -> Result<String, agentic_tools_core::ToolError> {
                let _ = (ctx, extra);
                Ok(input)
            }
        };
        let res = expand(quote!(), item);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("(input: T)"),
            "Expected guidance about parameters, got: {msg}"
        );
    }

    #[test]
    fn test_expand_rejects_wrong_second_param_type() {
        let item = quote! {
            async fn greet(input: String, extra: u32) -> Result<String, agentic_tools_core::ToolError> {
                let _ = extra;
                Ok(input)
            }
        };
        let res = expand(quote!(), item);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("ToolContext"),
            "Expected error about ToolContext, got: {msg}"
        );
    }

    #[test]
    fn test_expand_rejects_owned_toolcontext() {
        let item = quote! {
            async fn greet(
                input: String,
                ctx: agentic_tools_core::ToolContext,
            ) -> Result<String, agentic_tools_core::ToolError> {
                let _ = ctx;
                Ok(input)
            }
        };
        let res = expand(quote!(), item);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(
            msg.contains("ToolContext"),
            "Expected error about ToolContext ref, got: {msg}"
        );
    }

    #[test]
    fn test_expand_accepts_two_params_and_forwards_ctx() {
        let item = quote! {
            async fn greet(
                input: String,
                ctx: &agentic_tools_core::ToolContext,
            ) -> Result<String, agentic_tools_core::ToolError> {
                let _ = ctx;
                Ok(input)
            }
        };
        let expanded = expand(quote!(), item).expect("expected expansion to succeed");
        let s: String = expanded.to_string().split_whitespace().collect();

        // Verify clone-and-forward pattern in generated code
        assert!(s.contains("ctx.clone()"), "expected ctx.clone() in: {s}");
        assert!(s.contains("asyncmove"), "expected async move block in: {s}");
    }
}
