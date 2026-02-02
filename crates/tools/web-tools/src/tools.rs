//! Tool trait implementations and registry builder.

use std::sync::Arc;

use agentic_tools_core::ToolRegistry;
use agentic_tools_core::context::ToolContext;
use agentic_tools_core::error::ToolError;
use agentic_tools_core::tool::Tool;
use futures::future::BoxFuture;

use crate::WebTools;
use crate::types::{WebFetchInput, WebFetchOutput, WebSearchInput, WebSearchOutput};

// ============================================================================
// WebFetchTool
// ============================================================================

/// MCP tool for fetching web pages and converting to markdown.
#[derive(Clone)]
pub struct WebFetchTool {
    tools: Arc<WebTools>,
}

impl WebFetchTool {
    /// Create a new `WebFetchTool` with shared state.
    #[must_use]
    pub const fn new(tools: Arc<WebTools>) -> Self {
        Self { tools }
    }
}

impl Tool for WebFetchTool {
    type Input = WebFetchInput;
    type Output = WebFetchOutput;

    const NAME: &'static str = "web_fetch";
    const DESCRIPTION: &'static str = "Fetch a URL over HTTP and convert the page to clean Markdown with metadata. Default summarize=false; set summarize=true to generate a short Haiku summary (requires Anthropic credentials).";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move { crate::fetch::web_fetch(&tools, input).await })
    }
}

// ============================================================================
// WebSearchTool
// ============================================================================

/// MCP tool for semantic web search via Exa.
#[derive(Clone)]
pub struct WebSearchTool {
    tools: Arc<WebTools>,
}

impl WebSearchTool {
    /// Create a new `WebSearchTool` with shared state.
    #[must_use]
    pub const fn new(tools: Arc<WebTools>) -> Self {
        Self { tools }
    }
}

impl Tool for WebSearchTool {
    type Input = WebSearchInput;
    type Output = WebSearchOutput;

    const NAME: &'static str = "web_search";
    const DESCRIPTION: &'static str = "Semantic/neural web search (Exa). Use NATURAL LANGUAGE queries (questions/descriptions). Do NOT use keyword-stuffed, Google-style queries. Returns compact, citable result cards with URLs plus a short trimmed context to orient you.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move { crate::search::web_search(&tools, input).await })
    }
}

// ============================================================================
// Registry Builder
// ============================================================================

/// Build a `ToolRegistry` containing all web tools.
pub fn build_registry(tools: Arc<WebTools>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<WebFetchTool, ()>(WebFetchTool::new(tools.clone()))
        .register::<WebSearchTool, ()>(WebSearchTool::new(tools))
        .finish()
}
