//! Implementation of the #[tool] proc macro.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, ReturnType, Type, parse2};

/// Expand the #[tool] attribute macro.
pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let func: ItemFn = parse2(item)?;

    // Parse attributes
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    // Simple attribute parsing
    let attr_str = attr.to_string();
    for part in attr_str.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("name")
            && let Some(val) = rest.trim().strip_prefix('=')
        {
            let val = val.trim().trim_matches('"');
            name = Some(val.to_string());
        } else if let Some(rest) = part.strip_prefix("description")
            && let Some(val) = rest.trim().strip_prefix('=')
        {
            let val = val.trim().trim_matches('"');
            description = Some(val.to_string());
        }
    }

    let fn_ident = &func.sig.ident;
    let tool_name = name.unwrap_or_else(|| fn_ident.to_string());
    let tool_description = description.unwrap_or_default();

    // Create PascalCase struct name from function name
    let struct_name = to_pascal_case(&fn_ident.to_string());
    let tool_struct = format_ident!("{}Tool", struct_name);

    // Extract input type from first parameter
    let input_type: Type = func
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg {
                Some((*pat_type.ty).clone())
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "tool function must have an input parameter",
            )
        })?;

    // Extract output type from return type
    // Assume Result<Output, ToolError> and extract Output
    let output_type: Type = match &func.sig.output {
        ReturnType::Type(_, ty) => extract_result_ok_type(ty).unwrap_or_else(|| (**ty).clone()),
        ReturnType::Default => {
            return Err(syn::Error::new(
                Span::call_site(),
                "tool function must have a return type",
            ));
        }
    };

    let expanded = quote! {
        #func

        /// Auto-generated tool struct for [`#fn_ident`].
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
                _ctx: &agentic_tools_core::ToolContext,
            ) -> futures::future::BoxFuture<'static, Result<Self::Output, agentic_tools_core::ToolError>>
            {
                Box::pin(#fn_ident(input))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("get_comments"), "GetComments");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }
}
