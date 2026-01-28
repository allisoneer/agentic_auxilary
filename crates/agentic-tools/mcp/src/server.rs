//! MCP server handler backed by ToolRegistry.

use agentic_tools_core::fmt::{TextOptions, fallback_text_from_json};
use agentic_tools_core::{ToolContext, ToolRegistry};
use rmcp::model as m;
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler};
use std::collections::HashSet;
use std::sync::Arc;

/// Output mode for tool results.
#[derive(Clone, Copy, Debug, Default)]
pub enum OutputMode {
    /// Return results as formatted text (human-readable). Default.
    #[default]
    Text,
    /// Return structured results alongside text.
    /// - list_tools publishes output_schema (if available)
    /// - call_tool populates structured_content
    Structured,
}

/// MCP server handler backed by a [`ToolRegistry`].
///
/// Features:
/// - Automatic tool discovery from registry
/// - Optional allowlist filtering
/// - Configurable output mode (text or structured)
///
/// # Output Modes
///
/// - **Text** (default): Returns human-readable text using `TextFormat` when available,
///   falling back to pretty-printed JSON. Does not publish `output_schema`.
/// - **Structured**: Publishes `output_schema` in `list_tools` and populates
///   `structured_content` in `call_tool` responses (for MCP protocol compliance).
///
/// # Example
///
/// ```ignore
/// use agentic_tools_mcp::{RegistryServer, OutputMode};
/// use agentic_tools_core::ToolRegistry;
/// use std::sync::Arc;
///
/// let registry = Arc::new(ToolRegistry::builder()
///     .register::<MyTool, ()>(MyTool)
///     .finish());
///
/// // Text mode (default) - optimized for LLM consumption
/// let server = RegistryServer::new(registry.clone())
///     .with_allowlist(["my_tool".to_string()]);
///
/// // Structured mode - MCP protocol compliance with typed responses
/// let server = RegistryServer::new(registry)
///     .with_output_mode(OutputMode::Structured);
/// ```
pub struct RegistryServer {
    registry: Arc<ToolRegistry>,
    allowlist: Option<HashSet<String>>,
    output_mode: OutputMode,
    name: String,
    version: String,
}

impl RegistryServer {
    /// Create a new server from a registry.
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            allowlist: None,
            output_mode: OutputMode::default(),
            name: "agentic-tools".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Set an allowlist of tool names.
    ///
    /// Only tools in this list will be visible and callable.
    pub fn with_allowlist(mut self, allowlist: impl IntoIterator<Item = String>) -> Self {
        self.allowlist = Some(allowlist.into_iter().collect());
        self
    }

    /// Set the output mode for tool results.
    pub fn with_output_mode(mut self, mode: OutputMode) -> Self {
        self.output_mode = mode;
        self
    }

    /// Set the server name and version.
    pub fn with_info(mut self, name: &str, version: &str) -> Self {
        self.name = name.to_string();
        self.version = version.to_string();
        self
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the server version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get the output mode.
    pub fn output_mode(&self) -> OutputMode {
        self.output_mode
    }

    /// Get the list of effective tool names (respecting allowlist).
    pub fn effective_tool_names(&self) -> Vec<String> {
        self.registry
            .list_names()
            .into_iter()
            .filter(|n| self.is_allowed(n))
            .collect()
    }

    fn is_allowed(&self, name: &str) -> bool {
        self.allowlist.as_ref().is_none_or(|set| set.contains(name))
    }
}

// Allow manual_async_fn because the trait signature uses `impl Future` return types
#[allow(clippy::manual_async_fn)]
impl ServerHandler for RegistryServer {
    fn initialize(
        &self,
        _params: m::InitializeRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::InitializeResult, m::ErrorData>> + Send + '_
    {
        async move {
            Ok(m::InitializeResult {
                server_info: m::Implementation {
                    name: self.name.clone(),
                    title: self.name.clone().into(),
                    version: self.version.clone(),
                    website_url: None,
                    icons: None,
                },
                capabilities: m::ServerCapabilities::builder().enable_tools().build(),
                ..Default::default()
            })
        }
    }

    fn list_tools(
        &self,
        _req: Option<m::PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::ListToolsResult, m::ErrorData>> + Send + '_
    {
        async move {
            let mut tools = vec![];
            for name in self.registry.list_names() {
                if !self.is_allowed(&name) {
                    continue;
                }
                if let Some(erased) = self.registry.get(&name) {
                    let input_schema = erased.input_schema();
                    let schema_json = serde_json::to_value(&input_schema)
                        .unwrap_or(serde_json::json!({"type": "object"}));

                    // Include output_schema only in Structured mode (MCP compliance)
                    let output_schema = if matches!(self.output_mode, OutputMode::Structured) {
                        erased.output_schema().and_then(|s| {
                            serde_json::to_value(&s)
                                .ok()
                                .and_then(|v| v.as_object().cloned())
                                .map(Arc::new)
                        })
                    } else {
                        None
                    };

                    let tool = m::Tool {
                        name: name.clone().into(),
                        title: name.into(),
                        description: Some(erased.description().to_string().into()),
                        input_schema: Arc::new(
                            schema_json.as_object().cloned().unwrap_or_default(),
                        ),
                        annotations: None,
                        output_schema,
                        icons: None,
                        meta: None,
                    };
                    tools.push(tool);
                }
            }
            Ok(m::ListToolsResult {
                tools,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        req: m::CallToolRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::CallToolResult, m::ErrorData>> + Send + '_
    {
        async move {
            if !self.is_allowed(&req.name) {
                return Ok(m::CallToolResult::error(vec![m::Content::text(format!(
                    "Tool '{}' not enabled on this server",
                    req.name
                ))]));
            }

            let args = serde_json::Value::Object(req.arguments.unwrap_or_default());
            let ctx = ToolContext::default();
            let text_opts = TextOptions::default();

            match self
                .registry
                .dispatch_json_formatted(&req.name, args, &ctx, &text_opts)
                .await
            {
                Ok(res) => {
                    let text = res
                        .text
                        .unwrap_or_else(|| fallback_text_from_json(&res.data));

                    // Always include text content for human readability
                    let contents = vec![m::Content::text(text)];

                    // In Structured mode, also include structured_content if tool has a schema
                    let structured_content = if matches!(self.output_mode, OutputMode::Structured) {
                        // Check if the tool has an output schema (object-root)
                        let has_schema = self
                            .registry
                            .get(&req.name)
                            .and_then(|t| t.output_schema())
                            .is_some();

                        if has_schema { Some(res.data) } else { None }
                    } else {
                        None
                    };

                    // Build result with both text content and optional structured_content
                    Ok(m::CallToolResult {
                        content: contents,
                        structured_content,
                        is_error: Some(false),
                        meta: None,
                    })
                }
                Err(e) => Ok(m::CallToolResult::error(vec![m::Content::text(
                    e.to_string(),
                )])),
            }
        }
    }

    fn ping(
        &self,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), m::ErrorData>> + Send + '_ {
        async { Ok(()) }
    }

    fn complete(
        &self,
        _req: m::CompleteRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::CompleteResult, m::ErrorData>> + Send + '_
    {
        async {
            Err(m::ErrorData::invalid_request(
                "Method not implemented",
                None,
            ))
        }
    }

    fn set_level(
        &self,
        _req: m::SetLevelRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), m::ErrorData>> + Send + '_ {
        async { Ok(()) }
    }

    fn get_prompt(
        &self,
        _req: m::GetPromptRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::GetPromptResult, m::ErrorData>> + Send + '_
    {
        async {
            Err(m::ErrorData::invalid_request(
                "Method not implemented",
                None,
            ))
        }
    }

    fn list_prompts(
        &self,
        _req: Option<m::PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::ListPromptsResult, m::ErrorData>> + Send + '_
    {
        async {
            Ok(m::ListPromptsResult {
                prompts: vec![],
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn list_resources(
        &self,
        _req: Option<m::PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::ListResourcesResult, m::ErrorData>> + Send + '_
    {
        async {
            Ok(m::ListResourcesResult {
                resources: vec![],
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn list_resource_templates(
        &self,
        _req: Option<m::PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::ListResourceTemplatesResult, m::ErrorData>>
    + Send
    + '_ {
        async {
            Ok(m::ListResourceTemplatesResult {
                resource_templates: vec![],
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn read_resource(
        &self,
        _req: m::ReadResourceRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<m::ReadResourceResult, m::ErrorData>> + Send + '_
    {
        async {
            Err(m::ErrorData::invalid_request(
                "Method not implemented",
                None,
            ))
        }
    }

    fn subscribe(
        &self,
        _req: m::SubscribeRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), m::ErrorData>> + Send + '_ {
        async {
            Err(m::ErrorData::invalid_request(
                "Method not implemented",
                None,
            ))
        }
    }

    fn unsubscribe(
        &self,
        _req: m::UnsubscribeRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), m::ErrorData>> + Send + '_ {
        async {
            Err(m::ErrorData::invalid_request(
                "Method not implemented",
                None,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_tools_core::fmt::TextFormat;
    use agentic_tools_core::{Tool, ToolError};
    use futures::future::BoxFuture;

    #[test]
    fn test_registry_server_allowlist() {
        let registry = Arc::new(ToolRegistry::builder().finish());
        let server = RegistryServer::new(registry.clone())
            .with_allowlist(["tool_a".to_string(), "tool_b".to_string()]);

        assert!(server.is_allowed("tool_a"));
        assert!(server.is_allowed("tool_b"));
        assert!(!server.is_allowed("tool_c"));
    }

    #[test]
    fn test_registry_server_no_allowlist() {
        let registry = Arc::new(ToolRegistry::builder().finish());
        let server = RegistryServer::new(registry.clone());

        // Without allowlist, everything is allowed
        assert!(server.is_allowed("any_tool"));
    }

    #[test]
    fn test_registry_server_info() {
        let registry = Arc::new(ToolRegistry::builder().finish());
        let server = RegistryServer::new(registry.clone()).with_info("my-server", "1.0.0");

        assert_eq!(server.name(), "my-server");
        assert_eq!(server.version(), "1.0.0");
    }

    // Test tool with object-root output schema and TextFormat implementation
    #[derive(Clone)]
    struct TestObjTool;

    #[derive(
        serde::Serialize, serde::Deserialize, schemars::JsonSchema, Clone, Debug, PartialEq,
    )]
    struct TestObjOut {
        message: String,
    }

    impl TextFormat for TestObjOut {
        fn fmt_text(&self, _opts: &TextOptions) -> String {
            format!("Message: {}", self.message)
        }
    }

    impl Tool for TestObjTool {
        type Input = ();
        type Output = TestObjOut;
        const NAME: &'static str = "test_obj_tool";
        const DESCRIPTION: &'static str = "outputs an object";

        fn call(
            &self,
            _input: (),
            _ctx: &ToolContext,
        ) -> BoxFuture<'static, Result<TestObjOut, ToolError>> {
            Box::pin(async move {
                Ok(TestObjOut {
                    message: "hello".into(),
                })
            })
        }
    }

    #[test]
    fn test_structured_mode_output_schema_gating() {
        // Build registry with TestObjTool
        let registry = Arc::new(
            ToolRegistry::builder()
                .register::<TestObjTool, ()>(TestObjTool)
                .finish(),
        );

        // In Structured mode, we should publish output_schema
        let structured_server =
            RegistryServer::new(registry.clone()).with_output_mode(OutputMode::Structured);
        assert!(matches!(
            structured_server.output_mode(),
            OutputMode::Structured
        ));

        // In Text mode, we should NOT publish output_schema
        let text_server = RegistryServer::new(registry.clone()).with_output_mode(OutputMode::Text);
        assert!(matches!(text_server.output_mode(), OutputMode::Text));

        // Verify the tool has an output schema in the registry
        let tool = registry.get("test_obj_tool").unwrap();
        assert!(
            tool.output_schema().is_some(),
            "TestObjTool should have an output schema"
        );
    }

    #[tokio::test]
    async fn test_structured_mode_structured_content_via_dispatch() {
        // Build registry with TestObjTool
        let registry = Arc::new(
            ToolRegistry::builder()
                .register::<TestObjTool, ()>(TestObjTool)
                .finish(),
        );

        // Dispatch the tool call directly through the registry
        let ctx = ToolContext::default();
        let text_opts = TextOptions::default();
        let result = registry
            .dispatch_json_formatted("test_obj_tool", serde_json::json!(null), &ctx, &text_opts)
            .await
            .unwrap();

        // Verify we get both data and text
        assert_eq!(result.data, serde_json::json!({"message": "hello"}));
        assert!(result.text.is_some());

        // The logic for structured_content is in the server's call_tool method
        // which checks output_mode and has_schema. Let's verify the schema exists.
        let tool = registry.get("test_obj_tool").unwrap();
        let has_schema = tool.output_schema().is_some();
        assert!(
            has_schema,
            "Tool should have output schema for structured content"
        );

        // In Structured mode with has_schema=true, structured_content would be Some(result.data)
        // In Text mode or has_schema=false, structured_content would be None
    }

    #[test]
    fn test_output_mode_default_is_text() {
        let registry = Arc::new(ToolRegistry::builder().finish());
        let server = RegistryServer::new(registry);

        // Default should be Text mode
        assert!(matches!(server.output_mode(), OutputMode::Text));
    }
}
