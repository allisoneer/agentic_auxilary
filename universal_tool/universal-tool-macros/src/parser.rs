//! Parser for converting syn AST into our internal model representation.
//!
//! This module handles parsing of the universal_tool macros using darling
//! for clean attribute parsing and syn for AST traversal.

use darling::{FromAttributes, FromMeta, ast::NestedMeta};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, FnArg, GenericArgument, ImplItem, ItemImpl, LitStr, Pat, PatType, PathArguments,
    ReturnType, Type, TypePath, parse2,
};

use crate::model::*;
use syn::visit_mut::{self, VisitMut};

/// Parse the universal_tool_router attribute macro.
pub fn parse_router(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    // Parse the impl block
    let impl_block = parse2::<ItemImpl>(item)?;

    // Parse router attributes
    let router_attr = if attr.is_empty() {
        RouterAttr::default()
    } else {
        // Parse the attribute tokens into NestedMeta
        let nested_metas = NestedMeta::parse_meta_list(attr.clone())
            .map_err(|e| syn::Error::new_spanned(&attr, e))?;

        RouterAttr::from_list(&nested_metas).map_err(|e| syn::Error::new_spanned(&attr, e))?
    };

    // Convert to our internal model
    let router_def = parse_impl_to_router(&impl_block, router_attr)?;

    // Validate the model
    if let Err(errors) = router_def.validate() {
        let mut combined_error = None;
        for error in errors {
            let syn_error = syn::Error::new(error.span, &error.message);
            match &mut combined_error {
                None => combined_error = Some(syn_error),
                Some(e) => e.combine(syn_error),
            }
        }
        return Err(combined_error.unwrap());
    }

    // FEATURE SYSTEM DOCUMENTATION
    // ============================
    //
    // UTF uses feature propagation for a seamless user experience:
    //
    // 1. Users only depend on universal-tool-core with the features they want:
    //    ```toml
    //    [dependencies]
    //    universal-tool-core = { version = "0.1", features = ["rest"] }
    //    ```
    //
    // 2. The core crate's Cargo.toml propagates features to the macro crate:
    //    ```toml
    //    [features]
    //    rest = ["dep:axum", ..., "universal-tool-macros/rest"]
    //    ```
    //
    // 3. This parser checks cfg!(feature = "...") at COMPILE TIME of the macro crate,
    //    which means it detects features enabled on universal-tool-macros.
    //
    // 4. Only the code for enabled features is generated, avoiding unnecessary
    //    dependencies and compilation errors.
    //
    // This is the same pattern used by successful crates like serde, tokio, and diesel.
    // It provides the best user experience - users enable features in one place and
    // everything "just works".

    // Only generate CLI code if the cli feature is enabled on THIS crate
    let cli_methods = if cfg!(feature = "cli") {
        crate::codegen::cli::generate_cli_methods(&router_def)
    } else {
        TokenStream::new() // Use new() instead of quote! {}
    };

    // Only generate MCP code if the mcp feature is enabled on THIS crate
    let mcp_methods = if cfg!(feature = "mcp") {
        crate::codegen::mcp::generate_mcp_methods(&router_def)
    } else {
        TokenStream::new()
    };

    // For REST, we need to handle module generation separately
    let (rest_module, rest_methods) = if cfg!(feature = "rest") {
        crate::codegen::rest::generate_rest_methods_split(&router_def)
    } else {
        (TokenStream::new(), TokenStream::new())
    };

    // Strip universal_tool_param attributes from the impl block before returning
    let mut cleaned_impl_block = impl_block.clone();
    strip_param_attributes(&mut cleaned_impl_block);

    // Return the cleaned impl block plus generated methods
    // This preserves the original token structure exactly as the working version did
    let output = quote! {
        #rest_module

        #cleaned_impl_block

        #cli_methods
        #rest_methods
        #mcp_methods
    };

    // Debug: Print the generated code to stderr for inspection
    if std::env::var("UTF_DEBUG").is_ok() {
        eprintln!("Generated code:\n{}", output);
    }

    Ok(output)
}

/// Darling attribute structure for #[universal_tool_router(...)]
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct RouterAttr {
    /// OpenAPI tag for grouping endpoints
    openapi_tag: Option<String>,
    /// Base path for REST endpoints
    base_path: Option<String>,
    /// CLI-specific configuration
    cli: Option<RouterCliAttr>,
    /// REST-specific configuration
    rest: Option<RouterRestAttr>,
    /// MCP-specific configuration
    mcp: Option<RouterMcpAttr>,
}

/// Router-level CLI configuration
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct RouterCliAttr {
    /// CLI command name
    name: Option<String>,
    /// CLI command description
    description: Option<String>,
    /// Global output formats
    global_output_formats: Option<Vec<LitStr>>,
    /// Add standard global args
    standard_global_args: Option<bool>,
}

/// Router-level REST configuration
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct RouterRestAttr {
    /// Base prefix for all REST endpoints
    prefix: Option<String>,
}

/// Router-level MCP configuration
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct RouterMcpAttr {
    /// MCP server name
    name: Option<String>,
    /// MCP server version
    version: Option<String>,
}

/// Darling attribute structure for #[universal_tool(...)]
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct ToolAttr {
    /// Custom name for the tool (defaults to method name)
    name: Option<String>,
    /// Tool description
    description: String,
    /// Short description for CLI
    short: Option<String>,
    /// REST-specific configuration
    #[darling(default)]
    rest: Option<RestAttr>,
    /// MCP-specific configuration
    #[darling(default)]
    mcp: Option<McpAttr>,
    /// CLI-specific configuration
    #[darling(default)]
    cli: Option<CliAttr>,
}

/// REST configuration attributes
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct RestAttr {
    /// Custom path for this endpoint
    path: Option<String>,
    /// HTTP method (GET, POST, etc.)
    #[darling(default = "default_http_method")]
    method: String,
}

fn default_http_method() -> String {
    "POST".to_string()
}

/// MCP configuration attributes
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct McpAttr {
    /// Read-only hint
    read_only: Option<bool>,
    /// Destructive operation hint
    destructive: Option<bool>,
    /// Idempotent operation hint
    idempotent: Option<bool>,
    /// Open world hint - accepts additional parameters
    open_world: Option<bool>,
}

/// CLI configuration attributes
#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct CliAttr {
    /// Command name override
    name: Option<String>,
    /// Command aliases
    #[darling(multiple)]
    alias: Vec<String>,
    /// Hide from help
    #[darling(default)]
    hidden: bool,
    /// Output formats
    output_formats: Option<Vec<LitStr>>,
    /// Progress style
    progress_style: Option<String>,
    /// Supports stdin
    supports_stdin: Option<bool>,
    /// Supports stdout
    supports_stdout: Option<bool>,
    /// Confirmation message
    confirm: Option<String>,
    /// Interactive mode
    interactive: Option<bool>,
    /// Command path
    command_path: Option<Vec<LitStr>>,
}

/// Darling attribute structure for #[universal_tool_param(...)]
#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(universal_tool_param))]
struct ParamAttr {
    /// Parameter source (body, query, path, header)
    source: Option<String>,
    /// Parameter description
    description: Option<String>,
    /// Short flag for CLI
    short: Option<char>,
    /// Long name override for CLI
    long: Option<String>,
    /// Environment variable
    env: Option<String>,
    /// Default value
    default: Option<String>,
    /// Possible values
    possible_values: Option<Vec<LitStr>>,
    /// Multiple values allowed
    multiple: Option<bool>,
    /// Value delimiter
    delimiter: Option<char>,
    /// Completions hint
    completions: Option<String>,
}

/// Parse an impl block into our RouterDef model.
fn parse_impl_to_router(impl_block: &ItemImpl, router_attr: RouterAttr) -> syn::Result<RouterDef> {
    // Extract the struct type
    let struct_type = match &*impl_block.self_ty {
        Type::Path(type_path) => type_path.path.clone(),
        _ => {
            return Err(syn::Error::new_spanned(
                &impl_block.self_ty,
                "universal_tool_router can only be applied to named types",
            ));
        }
    };

    // Parse all tool methods
    let mut tools = Vec::new();
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            if let Some(tool) = parse_tool_method(method)? {
                tools.push(tool);
            }
        }
    }

    // Build the router definition
    Ok(RouterDef {
        struct_type,
        generics: if impl_block.generics.params.is_empty() {
            None
        } else {
            Some(impl_block.generics.clone())
        },
        tools,
        metadata: RouterMetadata {
            openapi_tag: router_attr.openapi_tag,
            base_path: router_attr
                .rest
                .as_ref()
                .and_then(|r| r.prefix.clone())
                .or(router_attr.base_path),
            cli_config: router_attr.cli.map(|c| crate::model::RouterCliConfig {
                name: c.name,
                description: c.description,
                global_output_formats: c
                    .global_output_formats
                    .map(|v| v.into_iter().map(|lit| lit.value()).collect())
                    .unwrap_or_default(),
                standard_global_args: c.standard_global_args.unwrap_or(false),
            }),
        },
    })
}

/// Parse a method that might be a tool.
fn parse_tool_method(method: &syn::ImplItemFn) -> syn::Result<Option<ToolDef>> {
    // Look for #[universal_tool] attribute
    let tool_attr = match find_tool_attribute(&method.attrs)? {
        Some(attr) => attr,
        None => return Ok(None), // No #[universal_tool] attribute found
    };

    let method_name = method.sig.ident.clone();
    let tool_name = tool_attr.name.unwrap_or_else(|| method_name.to_string());

    // Parse parameters
    let mut params = parse_parameters(&method.sig.inputs)?;

    // Check return type
    let return_type = match &method.sig.output {
        ReturnType::Default => {
            return Err(syn::Error::new_spanned(
                &method.sig,
                "Tool methods must have a return type",
            ));
        }
        ReturnType::Type(_, ty) => (**ty).clone(),
    };

    // Validate it returns Result<T, ToolError>
    validate_return_type(&return_type)?;

    // Extract description from doc comments if not provided
    let description = if tool_attr.description.is_empty() {
        extract_doc_comment(&method.attrs)
            .unwrap_or_else(|| format!("Execute {} operation", tool_name))
    } else {
        tool_attr.description
    };

    // Build metadata
    let metadata = ToolMetadata {
        description,
        short_description: tool_attr.short,
        rest_config: tool_attr.rest.map(|r| RestConfig {
            path: r.path,
            method: parse_http_method(&r.method).unwrap_or_default(),
        }),
        mcp_config: tool_attr.mcp.map(|m| McpConfig {
            annotations: McpAnnotations {
                read_only_hint: m.read_only,
                destructive_hint: m.destructive,
                idempotent_hint: m.idempotent,
                open_world_hint: m.open_world,
            },
        }),
        cli_config: tool_attr.cli.map(|c| CliConfig {
            name: c.name,
            aliases: c.alias,
            hidden: c.hidden,
            output_formats: c
                .output_formats
                .map(|v| v.into_iter().map(|lit| lit.value()).collect())
                .unwrap_or_default(),
            progress_style: c.progress_style,
            examples: vec![], // Examples in attributes are complex to parse with darling
            supports_stdin: c.supports_stdin.unwrap_or(false),
            supports_stdout: c.supports_stdout.unwrap_or(false),
            confirm: c.confirm,
            interactive: c.interactive.unwrap_or(false),
            command_path: c
                .command_path
                .map(|v| v.into_iter().map(|lit| lit.value()).collect())
                .unwrap_or_default(),
        }),
    };

    // Update parameter sources based on REST path
    if let Some(rest_config) = &metadata.rest_config {
        if let Some(path) = &rest_config.path {
            // Extract path parameter names from REST path (e.g., ":project_id" -> "project_id")
            let path_param_names: Vec<String> = path
                .split('/')
                .filter(|segment| segment.starts_with(':'))
                .map(|segment| segment[1..].to_string())
                .collect();

            // Update the source for any matching parameters
            for param in &mut params {
                if path_param_names.contains(&param.name.to_string()) {
                    param.source = ParamSource::Path;
                }
            }
        }
    }

    Ok(Some(ToolDef {
        method_name,
        tool_name,
        params,
        return_type,
        metadata,
        is_async: method.sig.asyncness.is_some(),
        visibility: method.vis.clone(),
    }))
}

/// Find and parse the #[universal_tool] attribute.
fn find_tool_attribute(attrs: &[Attribute]) -> syn::Result<Option<ToolAttr>> {
    for attr in attrs {
        if attr.path().is_ident("universal_tool") {
            let meta = &attr.meta;
            return Ok(Some(
                ToolAttr::from_meta(meta).map_err(|e| syn::Error::new_spanned(attr, e))?,
            ));
        }
    }
    Ok(None)
}

/// Parse function parameters into our model.
fn parse_parameters(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
) -> syn::Result<Vec<ParamDef>> {
    let mut params = Vec::new();

    for arg in inputs {
        match arg {
            FnArg::Receiver(_) => {
                // Skip &self - it's not a tool parameter
            }
            FnArg::Typed(pat_type) => {
                params.push(parse_typed_param(pat_type)?);
            }
        }
    }

    Ok(params)
}

/// Parse a typed parameter.
fn parse_typed_param(pat_type: &PatType) -> syn::Result<ParamDef> {
    // Extract parameter name
    let name = match &*pat_type.pat {
        Pat::Ident(pat_ident) => pat_ident.ident.clone(),
        _ => {
            return Err(syn::Error::new_spanned(
                pat_type,
                "Tool parameters must be simple identifiers",
            ));
        }
    };

    // Parse parameter attributes
    let param_attr = ParamAttr::from_attributes(&pat_type.attrs)
        .map_err(|e| syn::Error::new_spanned(pat_type, e))?;

    // Parse parameter source
    let source = if let Some(source_str) = param_attr.source {
        match source_str.as_str() {
            "body" => ParamSource::Body,
            "query" => ParamSource::Query,
            "path" => ParamSource::Path,
            "header" => ParamSource::Header,
            _ => {
                return Err(syn::Error::new_spanned(
                    pat_type,
                    format!(
                        "Invalid parameter source: {}. Must be one of: body, query, path, header",
                        source_str
                    ),
                ));
            }
        }
    } else {
        ParamSource::default()
    };

    // Check if type is Option<T>
    let is_optional = is_option_type(&pat_type.ty);

    Ok(ParamDef {
        name,
        ty: (*pat_type.ty).clone(),
        source,
        is_optional,
        metadata: ParamMetadata {
            description: param_attr.description,
            short: param_attr.short,
            long: param_attr.long,
            env: param_attr.env,
            default: param_attr.default,
            possible_values: param_attr
                .possible_values
                .map(|v| v.into_iter().map(|lit| lit.value()).collect())
                .unwrap_or_default(),
            completions: param_attr.completions,
            multiple: param_attr.multiple.unwrap_or(false),
            delimiter: param_attr.delimiter,
        },
    })
}

/// Check if a type is Option<T>.
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.first() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Validate that the return type is Result<T, ToolError>.
fn validate_return_type(ty: &Type) -> syn::Result<()> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            if segment.ident != "Result" {
                return Err(syn::Error::new_spanned(
                    ty,
                    "Tool methods must return Result<T, ToolError>",
                ));
            }

            // Check that it has two type arguments
            if let PathArguments::AngleBracketed(args) = &segment.arguments {
                if args.args.len() != 2 {
                    return Err(syn::Error::new_spanned(
                        ty,
                        "Result must have exactly two type parameters: Result<T, ToolError>",
                    ));
                }

                // Check that the error type is ToolError
                if let Some(GenericArgument::Type(error_type)) = args.args.iter().nth(1) {
                    if !is_tool_error_type(error_type) {
                        return Err(syn::Error::new_spanned(
                            error_type,
                            "Tool methods must return Result<T, ToolError>. The error type must be ToolError.",
                        ));
                    }
                }
            } else {
                return Err(syn::Error::new_spanned(
                    ty,
                    "Result must have type parameters: Result<T, ToolError>",
                ));
            }

            return Ok(());
        }
    }

    Err(syn::Error::new_spanned(
        ty,
        "Tool methods must return Result<T, ToolError>",
    ))
}

/// Check if a type is ToolError (or a path ending in ToolError).
fn is_tool_error_type(ty: &Type) -> bool {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            return segment.ident == "ToolError";
        }
    }
    false
}

/// Parse HTTP method string.
fn parse_http_method(method: &str) -> Option<HttpMethod> {
    match method.to_uppercase().as_str() {
        "GET" => Some(HttpMethod::Get),
        "POST" => Some(HttpMethod::Post),
        "PUT" => Some(HttpMethod::Put),
        "DELETE" => Some(HttpMethod::Delete),
        "PATCH" => Some(HttpMethod::Patch),
        _ => None,
    }
}

/// Extract documentation from doc comment attributes.
fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let mut docs = Vec::new();

    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(lit) = &meta.value {
                    if let syn::Lit::Str(s) = &lit.lit {
                        let line = s.value();
                        // Remove leading space if present (rustdoc convention)
                        let line = line.strip_prefix(' ').unwrap_or(&line);
                        docs.push(line.to_string());
                    }
                }
            }
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

/// Strip universal_tool_param attributes from function parameters
fn strip_param_attributes(impl_block: &mut ItemImpl) {
    struct ParamAttributeStripper;

    impl VisitMut for ParamAttributeStripper {
        fn visit_fn_arg_mut(&mut self, arg: &mut FnArg) {
            if let FnArg::Typed(pat_type) = arg {
                // Remove universal_tool_param attributes
                pat_type
                    .attrs
                    .retain(|attr| !attr.path().is_ident("universal_tool_param"));
            }
            // Continue visiting nested items
            visit_mut::visit_fn_arg_mut(self, arg);
        }
    }

    let mut stripper = ParamAttributeStripper;
    stripper.visit_item_impl_mut(impl_block);
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::parse_quote;

    #[test]
    fn test_parse_simple_router() {
        let input = quote! {
            impl MyTools {
                #[universal_tool(description = "Add two numbers")]
                pub async fn add(&self, a: i32, b: i32) -> Result<i32, ToolError> {
                    Ok(a + b)
                }
            }
        };

        let result = parse_router(TokenStream::new(), input);
        assert!(result.is_ok(), "Failed to parse simple router");
    }

    #[test]
    fn test_validate_return_type() {
        // Test valid return type
        let valid_type: Type = syn::parse_quote!(Result<i32, ToolError>);
        assert!(validate_return_type(&valid_type).is_ok());

        // Test invalid return type (not Result)
        let invalid_type: Type = syn::parse_quote!(i32);
        assert!(validate_return_type(&invalid_type).is_err());

        // Test Result with wrong error type
        let wrong_error: Type = syn::parse_quote!(Result<i32, std::io::Error>);
        assert!(validate_return_type(&wrong_error).is_err());
    }

    #[test]
    fn test_is_option_type() {
        let opt_type: Type = syn::parse_quote!(Option<String>);
        assert!(is_option_type(&opt_type));

        let non_opt_type: Type = syn::parse_quote!(String);
        assert!(!is_option_type(&non_opt_type));
    }

    #[test]
    fn test_parse_http_method() {
        assert_eq!(parse_http_method("GET"), Some(HttpMethod::Get));
        assert_eq!(parse_http_method("post"), Some(HttpMethod::Post));
        assert_eq!(parse_http_method("PUT"), Some(HttpMethod::Put));
        assert_eq!(parse_http_method("DELETE"), Some(HttpMethod::Delete));
        assert_eq!(parse_http_method("PATCH"), Some(HttpMethod::Patch));
        assert_eq!(parse_http_method("INVALID"), None);
    }

    #[test]
    fn test_extract_doc_comment() {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[doc = " This is a doc comment"]),
            syn::parse_quote!(#[doc = " with multiple lines"]),
        ];

        let doc = extract_doc_comment(&attrs);
        assert_eq!(
            doc,
            Some("This is a doc comment\nwith multiple lines".to_string())
        );
    }
}
