//! Tool registry for dynamic dispatch and type-safe native calls.

use crate::context::ToolContext;
use crate::error::ToolError;
use crate::fmt::{TextFormat, TextOptions};
use crate::schema::mcp_schema;
use crate::tool::{Tool, ToolCodec};
use futures::future::BoxFuture;
use schemars::Schema;
use serde_json::Value;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::sync::Arc;

/// Result from dispatch_json_formatted containing both JSON data and optional text.
#[derive(Debug, Clone)]
pub struct FormattedResult {
    /// The JSON-serialized output data.
    pub data: Value,
    /// Human-readable text representation. None if no TextFormat implementation exists
    /// and fallback wasn't requested.
    pub text: Option<String>,
}

/// Type-erased tool for dynamic dispatch.
pub trait ErasedTool: Send + Sync {
    /// Get the tool's name.
    fn name(&self) -> &'static str;

    /// Get the tool's description.
    fn description(&self) -> &'static str;

    /// Get the input JSON schema.
    fn input_schema(&self) -> Schema;

    /// Get the output JSON schema (if available).
    fn output_schema(&self) -> Option<Schema>;

    /// Call the tool with JSON arguments.
    fn call_json(
        &self,
        args: Value,
        ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Value, ToolError>>;

    /// Call the tool with JSON arguments, returning both JSON data and formatted text.
    ///
    /// This method enables dual output for MCP and NAPI servers. The text is derived
    /// from the tool's TextFormat implementation if available, otherwise it falls back
    /// to pretty-printed JSON.
    fn call_json_formatted(
        &self,
        args: Value,
        ctx: &ToolContext,
        text_opts: &TextOptions,
    ) -> BoxFuture<'static, Result<FormattedResult, ToolError>>;

    /// Get the TypeId for type-safe handle retrieval.
    fn type_id(&self) -> TypeId;
}

/// Registry of tools for dynamic dispatch and type-safe native calls.
pub struct ToolRegistry {
    map: HashMap<String, Arc<dyn ErasedTool>>,
    by_type: HashMap<TypeId, String>,
}

impl ToolRegistry {
    /// Create a new registry builder.
    pub fn builder() -> ToolRegistryBuilder {
        ToolRegistryBuilder::default()
    }

    /// List all tool names in the registry.
    pub fn list_names(&self) -> Vec<String> {
        self.map.keys().cloned().collect()
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn ErasedTool>> {
        self.map.get(name)
    }

    /// Create a subset registry containing only the specified tools.
    ///
    /// Tools not found in the registry are silently ignored.
    pub fn subset<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> ToolRegistry {
        let allowed: HashSet<&str> = names.into_iter().collect();

        // Copy the allowed entries into the new map
        let mut map = HashMap::new();
        for (k, v) in &self.map {
            if allowed.contains(k.as_str()) {
                map.insert(k.clone(), v.clone());
            }
        }

        // Reuse original TypeIds from by_type (don't recompute via trait object
        // to avoid cross-crate monomorphization issues with TypeId)
        let mut by_type = HashMap::new();
        for (type_id, name) in &self.by_type {
            if allowed.contains(name.as_str()) {
                by_type.insert(*type_id, name.clone());
            }
        }

        ToolRegistry { map, by_type }
    }

    /// Dispatch a tool call using JSON arguments.
    pub async fn dispatch_json(
        &self,
        name: &str,
        args: Value,
        ctx: &ToolContext,
    ) -> Result<Value, ToolError> {
        let entry = self
            .map
            .get(name)
            .ok_or_else(|| ToolError::invalid_input(format!("Unknown tool: {}", name)))?;
        entry.call_json(args, ctx).await
    }

    /// Dispatch a tool call using JSON arguments, returning both JSON data and formatted text.
    ///
    /// This method enables dual output for MCP and NAPI servers. The text is derived
    /// from the tool's TextFormat implementation if available, otherwise it falls back
    /// to pretty-printed JSON.
    pub async fn dispatch_json_formatted(
        &self,
        name: &str,
        args: Value,
        ctx: &ToolContext,
        text_opts: &TextOptions,
    ) -> Result<FormattedResult, ToolError> {
        let entry = self
            .map
            .get(name)
            .ok_or_else(|| ToolError::invalid_input(format!("Unknown tool: {}", name)))?;
        entry.call_json_formatted(args, ctx, text_opts).await
    }

    /// Get a type-safe handle for calling a tool natively (zero JSON).
    ///
    /// Returns an error if the tool type is not registered.
    pub fn handle<T: Tool>(&self) -> Result<ToolHandle<T>, ToolError> {
        let type_id = TypeId::of::<T>();
        self.by_type.get(&type_id).ok_or_else(|| {
            ToolError::invalid_input(format!(
                "Tool type not registered: {}",
                std::any::type_name::<T>()
            ))
        })?;
        Ok(ToolHandle {
            _marker: PhantomData,
        })
    }

    /// Check if a tool is registered by name.
    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Clone and return erased tool entries (Arc) for composition.
    ///
    /// This enables merging multiple registries by iterating over their
    /// erased tool entries and re-registering them in a new registry.
    pub fn iter_erased(&self) -> Vec<Arc<dyn ErasedTool>> {
        self.map.values().cloned().collect()
    }

    /// Merge multiple registries into one.
    ///
    /// Later entries with duplicate names overwrite earlier ones.
    /// This is useful for composing domain-specific registries into
    /// a unified registry.
    pub fn merge_all(regs: impl IntoIterator<Item = ToolRegistry>) -> ToolRegistry {
        let mut builder = ToolRegistry::builder();
        for reg in regs {
            for erased in reg.iter_erased() {
                builder = builder.register_erased(erased);
            }
        }
        builder.finish()
    }
}

/// Builder for constructing a [`ToolRegistry`].
#[derive(Default)]
pub struct ToolRegistryBuilder {
    items: Vec<(String, TypeId, Arc<dyn ErasedTool>)>,
}

impl ToolRegistryBuilder {
    /// Register a tool with its codec.
    ///
    /// Use `()` as the codec when the tool's Input/Output types
    /// already implement serde and schemars traits.
    ///
    /// The tool's output type must implement [`TextFormat`] for human-readable
    /// formatting. Types can override `fmt_text()` for custom formatting, or
    /// use the default which produces pretty-printed JSON.
    pub fn register<T, C>(mut self, tool: T) -> Self
    where
        T: Tool + Clone + 'static,
        C: ToolCodec<T> + 'static,
        T::Output: TextFormat,
    {
        struct Impl<T: Tool + Clone, C: ToolCodec<T>> {
            tool: T,
            _codec: PhantomData<C>,
        }

        impl<T: Tool + Clone, C: ToolCodec<T>> ErasedTool for Impl<T, C>
        where
            T::Output: TextFormat,
        {
            fn name(&self) -> &'static str {
                T::NAME
            }

            fn description(&self) -> &'static str {
                T::DESCRIPTION
            }

            fn input_schema(&self) -> Schema {
                mcp_schema::cached_schema_for::<C::WireIn>()
                    .as_ref()
                    .clone()
            }

            fn output_schema(&self) -> Option<Schema> {
                match mcp_schema::cached_output_schema_for::<C::WireOut>() {
                    Ok(arc) => Some(arc.as_ref().clone()),
                    Err(_) => None,
                }
            }

            fn call_json(
                &self,
                args: Value,
                ctx: &ToolContext,
            ) -> BoxFuture<'static, Result<Value, ToolError>> {
                let wire_in: Result<C::WireIn, _> = serde_json::from_value(args);
                let ctx = ctx.clone();
                let tool = self.tool.clone();

                match wire_in {
                    Err(e) => Box::pin(async move { Err(ToolError::invalid_input(e.to_string())) }),
                    Ok(wire) => match C::decode(wire) {
                        Err(e) => Box::pin(async move { Err(e) }),
                        Ok(native_in) => {
                            let fut = tool.call(native_in, &ctx);
                            Box::pin(async move {
                                let out = fut.await?;
                                let wired = C::encode(out)?;
                                serde_json::to_value(wired)
                                    .map_err(|e| ToolError::internal(e.to_string()))
                            })
                        }
                    },
                }
            }

            fn call_json_formatted(
                &self,
                args: Value,
                ctx: &ToolContext,
                text_opts: &TextOptions,
            ) -> BoxFuture<'static, Result<FormattedResult, ToolError>> {
                let wire_in: Result<C::WireIn, _> = serde_json::from_value(args);
                let ctx = ctx.clone();
                let tool = self.tool.clone();
                let text_opts = text_opts.clone();

                match wire_in {
                    Err(e) => Box::pin(async move { Err(ToolError::invalid_input(e.to_string())) }),
                    Ok(wire) => match C::decode(wire) {
                        Err(e) => Box::pin(async move { Err(e) }),
                        Ok(native_in) => {
                            let fut = tool.call(native_in, &ctx);
                            Box::pin(async move {
                                let out = fut.await?;
                                // Format text from the native output using TextFormat
                                let text = out.fmt_text(&text_opts);
                                // Then encode to wire and JSON-serialize for data
                                let wired = C::encode(out)?;
                                let data = serde_json::to_value(&wired)
                                    .map_err(|e| ToolError::internal(e.to_string()))?;
                                Ok(FormattedResult {
                                    data,
                                    text: Some(text),
                                })
                            })
                        }
                    },
                }
            }

            fn type_id(&self) -> TypeId {
                TypeId::of::<T>()
            }
        }

        let erased: Arc<dyn ErasedTool> = Arc::new(Impl::<T, C> {
            tool,
            _codec: PhantomData,
        });
        self.items
            .push((T::NAME.to_string(), TypeId::of::<T>(), erased));
        self
    }

    /// Register an already-erased tool entry.
    ///
    /// This enables merging registries by iterating over their erased tools
    /// and re-registering them without needing the concrete tool types.
    pub fn register_erased(mut self, erased: Arc<dyn ErasedTool>) -> Self {
        let name = erased.name().to_string();
        let type_id = erased.type_id();
        self.items.push((name, type_id, erased));
        self
    }

    /// Build the registry from registered tools.
    pub fn finish(self) -> ToolRegistry {
        let mut map = HashMap::new();
        let mut by_type = HashMap::new();
        for (name, type_id, erased) in self.items {
            by_type.insert(type_id, name.clone());
            map.insert(name, erased);
        }
        ToolRegistry { map, by_type }
    }
}

/// Type-safe handle for calling a tool natively without JSON serialization.
///
/// Obtained from [`ToolRegistry::handle`].
pub struct ToolHandle<T: Tool> {
    _marker: PhantomData<T>,
}

impl<T: Tool> ToolHandle<T> {
    /// Call the tool directly with native types (zero JSON serialization).
    pub async fn call(
        &self,
        tool: &T,
        input: T::Input,
        ctx: &ToolContext,
    ) -> Result<T::Output, ToolError> {
        tool.call(input, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestTool;

    impl Tool for TestTool {
        type Input = String;
        type Output = String;
        const NAME: &'static str = "test_tool";
        const DESCRIPTION: &'static str = "A test tool";

        fn call(
            &self,
            input: Self::Input,
            _ctx: &ToolContext,
        ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
            Box::pin(async move { Ok(format!("Hello, {}!", input)) })
        }
    }

    #[test]
    fn test_registry_builder() {
        let registry = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        assert!(registry.contains("test_tool"));
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_registry_list_names() {
        let registry = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        let names = registry.list_names();
        assert_eq!(names, vec!["test_tool"]);
    }

    #[test]
    fn test_registry_subset() {
        let registry = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        let subset = registry.subset(["test_tool"]);
        assert!(subset.contains("test_tool"));

        let empty_subset = registry.subset(["nonexistent"]);
        assert!(empty_subset.is_empty());
    }

    #[test]
    fn test_tool_handle() {
        let registry = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        let handle = registry.handle::<TestTool>();
        assert!(handle.is_ok());
    }

    #[tokio::test]
    async fn test_dispatch_json_formatted() {
        let registry = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        let ctx = ToolContext::default();
        let args = serde_json::json!("World");
        let opts = TextOptions::default();

        let result = registry
            .dispatch_json_formatted("test_tool", args, &ctx, &opts)
            .await;

        assert!(result.is_ok());
        let formatted = result.unwrap();
        assert_eq!(formatted.data, serde_json::json!("Hello, World!"));
        assert!(formatted.text.is_some());
        // Text should be pretty-printed JSON
        assert!(formatted.text.unwrap().contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_dispatch_json_formatted_unknown_tool() {
        let registry = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        let ctx = ToolContext::default();
        let args = serde_json::json!("test");
        let opts = TextOptions::default();

        let result = registry
            .dispatch_json_formatted("nonexistent", args, &ctx, &opts)
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_iter_erased() {
        let registry = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        let erased = registry.iter_erased();
        assert_eq!(erased.len(), 1);
        assert_eq!(erased[0].name(), "test_tool");
    }

    #[test]
    fn test_register_erased_roundtrip() {
        // Create a registry with a tool
        let r1 = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        // Extract erased tool and re-register
        let erased = r1.iter_erased().into_iter().next().unwrap();
        let r2 = ToolRegistry::builder().register_erased(erased).finish();

        // Verify the tool was re-registered correctly
        assert_eq!(r2.len(), 1);
        assert!(r2.contains("test_tool"));
        assert_eq!(r2.get("test_tool").unwrap().name(), "test_tool");
    }

    #[test]
    fn test_merge_all_combines_registries() {
        // Create two registries with the same tool (simulating domain registries)
        let r1 = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();
        let r2 = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        // Merge them
        let merged = ToolRegistry::merge_all(vec![r1, r2]);

        // Duplicate names should result in last-wins (still only one tool)
        assert_eq!(merged.len(), 1);
        assert!(merged.contains("test_tool"));
    }

    #[test]
    fn test_merge_all_empty() {
        let merged = ToolRegistry::merge_all(Vec::<ToolRegistry>::new());
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_all_preserves_subset() {
        let r1 = ToolRegistry::builder()
            .register::<TestTool, ()>(TestTool)
            .finish();

        let merged = ToolRegistry::merge_all(vec![r1]);
        let subset = merged.subset(["test_tool"]);

        assert_eq!(subset.len(), 1);
        assert!(subset.contains("test_tool"));
    }
}
