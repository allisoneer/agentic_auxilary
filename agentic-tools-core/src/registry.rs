//! Tool registry for dynamic dispatch and type-safe native calls.

use crate::context::ToolContext;
use crate::error::ToolError;
use crate::tool::{Tool, ToolCodec};
use futures::future::BoxFuture;
use schemars::schema::RootSchema;
use serde_json::Value;
use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::sync::Arc;

/// Type-erased tool for dynamic dispatch.
pub trait ErasedTool: Send + Sync {
    /// Get the tool's name.
    fn name(&self) -> &'static str;

    /// Get the tool's description.
    fn description(&self) -> &'static str;

    /// Get the input JSON schema.
    fn input_schema(&self) -> RootSchema;

    /// Get the output JSON schema (if available).
    fn output_schema(&self) -> Option<RootSchema>;

    /// Call the tool with JSON arguments.
    fn call_json(
        &self,
        args: Value,
        ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Value, ToolError>>;

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
        let mut map = HashMap::new();
        let mut by_type = HashMap::new();
        for (k, v) in &self.map {
            if allowed.contains(k.as_str()) {
                map.insert(k.clone(), v.clone());
                by_type.insert(v.type_id(), k.clone());
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
    pub fn register<T, C>(mut self, tool: T) -> Self
    where
        T: Tool + Clone + 'static,
        C: ToolCodec<T> + 'static,
    {
        struct Impl<T: Tool + Clone, C: ToolCodec<T>> {
            tool: T,
            _codec: PhantomData<C>,
        }

        impl<T: Tool + Clone, C: ToolCodec<T>> ErasedTool for Impl<T, C> {
            fn name(&self) -> &'static str {
                T::NAME
            }

            fn description(&self) -> &'static str {
                T::DESCRIPTION
            }

            fn input_schema(&self) -> RootSchema {
                schemars::schema_for!(C::WireIn)
            }

            fn output_schema(&self) -> Option<RootSchema> {
                Some(schemars::schema_for!(C::WireOut))
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
}
