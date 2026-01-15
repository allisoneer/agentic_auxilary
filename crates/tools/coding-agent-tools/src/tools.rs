//! Tool wrappers for coding_agent_tools using agentic-tools-core.
//!
//! Each tool delegates to the corresponding method on [`CodingAgentTools`].

use crate::types::{
    AgentLocation, AgentOutput, AgentType, Depth, GlobOutput, GrepOutput, LsOutput, OutputMode,
    Show, SortOrder,
};
use crate::{CodingAgentTools, just};
use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Ls Tool
// ============================================================================

/// Input for the ls tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LsInput {
    /// Directory path (absolute or relative to cwd)
    #[serde(default)]
    pub path: Option<String>,
    /// Traversal depth: 0=header, 1=children (default), 2-10=tree
    #[serde(default)]
    pub depth: Option<Depth>,
    /// Filter: 'all' (default), 'files', or 'dirs'
    #[serde(default)]
    pub show: Option<Show>,
    /// Additional glob patterns to ignore
    #[serde(default)]
    pub ignore: Option<Vec<String>>,
    /// Include hidden files (default: false)
    #[serde(default)]
    pub hidden: Option<bool>,
}

/// Tool for listing files and directories.
#[derive(Clone)]
pub struct LsTool {
    tools: Arc<CodingAgentTools>,
}

impl LsTool {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self { tools }
    }
}

impl Tool for LsTool {
    type Input = LsInput;
    type Output = LsOutput;
    const NAME: &'static str = "ls";
    const DESCRIPTION: &'static str = "List files and directories. Depth: 0=header only, 1=children (default), 2-10=tree. Filter with show='files'|'dirs'|'all'. Gitignore-aware. For shallow queries, call with same params again for next page.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move {
            tools
                .ls(
                    input.path,
                    input.depth,
                    input.show,
                    input.ignore,
                    input.hidden,
                )
                .await
        })
    }
}

// ============================================================================
// SpawnAgent Tool
// ============================================================================

/// Input for the spawn_agent tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SpawnAgentInput {
    /// Agent type: 'locator' (fast discovery, haiku) or 'analyzer' (deep analysis, sonnet). Default: locator
    #[serde(default)]
    pub agent_type: Option<AgentType>,
    /// Location: 'codebase'|'thoughts'|'references'|'web'. Default: codebase
    #[serde(default)]
    pub location: Option<AgentLocation>,
    /// Task to perform; plain language question/instructions for the subagent
    pub query: String,
}

/// Tool for spawning Claude subagents.
#[derive(Clone)]
pub struct SpawnAgentTool {
    tools: Arc<CodingAgentTools>,
}

impl SpawnAgentTool {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self { tools }
    }
}

impl Tool for SpawnAgentTool {
    type Input = SpawnAgentInput;
    type Output = AgentOutput;
    const NAME: &'static str = "spawn_agent";
    const DESCRIPTION: &'static str = "Spawn a Claude subagent for discovery or deep analysis. Returns a single text response; no side effects.

Agent types:
- locator (haiku): Finds WHERE things are. Fast, shallow discovery via grep/glob/ls. Returns file paths grouped by purpose. Cannot read file contents deeply.
- analyzer (sonnet): Explains HOW things work. Reads files, traces data flow, provides technical analysis. Must cite file:line for all claims.

Locations:
- codebase: Current repository. Paths are repo-relative.
- thoughts: Active branch documents (research/plans/artifacts). Uses list_active_documents for discovery.
- references: Cloned reference repos. Paths start with references/{org}/{repo}/.
- web: Internet search. Returns URLs with quotes and source attribution.

When to use:
- Use locator when you need to find files/resources but don't yet know where they are.
- Use analyzer when you need to understand implementation details or extract specific information with citations.
- Use thoughts/references locations when the answer likely exists in existing documentation or external examples.
- Use web when you need external documentation, API references, or information not in the codebase.

When NOT to use:
- If you already know the file path, use Read directly.
- If you need a simple directory listing, use ls.
- For pattern matching in known locations, use search_grep or search_glob.
- This tool cannot write files or make changes.

Usage notes:
- Provide clear, specific queries. The subagent is stateless and receives no prior context.
- Locator returns locations only; use analyzer or Read for content.
- Multiple spawn_agent calls can run in parallel for independent queries.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move {
            tools
                .spawn_agent(input.agent_type, input.location, input.query)
                .await
        })
    }
}

// ============================================================================
// SearchGrep Tool
// ============================================================================

/// Input for the search_grep tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchGrepInput {
    /// Regex pattern to search for
    pub pattern: String,
    /// Directory path (absolute or relative to cwd)
    #[serde(default)]
    pub path: Option<String>,
    /// Output mode: 'files' (default), 'content', or 'count'
    #[serde(default)]
    pub mode: Option<OutputMode>,
    /// Include-only glob patterns (files to consider)
    #[serde(default)]
    pub globs: Option<Vec<String>>,
    /// Additional glob patterns to ignore (exclude)
    #[serde(default)]
    pub ignore: Option<Vec<String>>,
    /// Include hidden files (default: false)
    #[serde(default)]
    pub include_hidden: Option<bool>,
    /// Case-insensitive matching (default: false)
    #[serde(default)]
    pub case_insensitive: Option<bool>,
    /// Allow '.' to match newlines; patterns may span lines (default: false)
    #[serde(default)]
    pub multiline: Option<bool>,
    /// Show line numbers in content mode (default: true)
    #[serde(default)]
    pub line_numbers: Option<bool>,
    /// Context lines before and after matches (overridden by context_before/after if provided)
    #[serde(default)]
    pub context: Option<u32>,
    /// Context lines before match
    #[serde(default)]
    pub context_before: Option<u32>,
    /// Context lines after match
    #[serde(default)]
    pub context_after: Option<u32>,
    /// Search binary files as text (default: false)
    #[serde(default)]
    pub include_binary: Option<bool>,
    /// Max results to return (default: 200, capped at 1000)
    #[serde(default)]
    pub head_limit: Option<usize>,
    /// Skip the first N results (default: 0)
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Tool for regex-based code search.
#[derive(Clone)]
pub struct SearchGrepTool {
    tools: Arc<CodingAgentTools>,
}

impl SearchGrepTool {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self { tools }
    }
}

impl Tool for SearchGrepTool {
    type Input = SearchGrepInput;
    type Output = GrepOutput;
    const NAME: &'static str = "search_grep";
    const DESCRIPTION: &'static str = "Regex-based search. Modes: files (default), content, count. Stateless pagination via head_limit+offset.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move {
            tools
                .search_grep(
                    input.pattern,
                    input.path,
                    input.mode,
                    input.globs,
                    input.ignore,
                    input.include_hidden,
                    input.case_insensitive,
                    input.multiline,
                    input.line_numbers,
                    input.context,
                    input.context_before,
                    input.context_after,
                    input.include_binary,
                    input.head_limit,
                    input.offset,
                )
                .await
        })
    }
}

// ============================================================================
// SearchGlob Tool
// ============================================================================

/// Input for the search_glob tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SearchGlobInput {
    /// Glob pattern to match against (e.g., '**/*.rs')
    pub pattern: String,
    /// Directory path (absolute or relative to cwd)
    #[serde(default)]
    pub path: Option<String>,
    /// Additional glob patterns to ignore (exclude)
    #[serde(default)]
    pub ignore: Option<Vec<String>>,
    /// Include hidden files (default: false)
    #[serde(default)]
    pub include_hidden: Option<bool>,
    /// Sort order: 'name' (default) or 'mtime' (newest first)
    #[serde(default)]
    pub sort: Option<SortOrder>,
    /// Max results to return (default: 500, capped at 1000)
    #[serde(default)]
    pub head_limit: Option<usize>,
    /// Skip the first N results (default: 0)
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Tool for glob-based file matching.
#[derive(Clone)]
pub struct SearchGlobTool {
    tools: Arc<CodingAgentTools>,
}

impl SearchGlobTool {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self { tools }
    }
}

impl Tool for SearchGlobTool {
    type Input = SearchGlobInput;
    type Output = GlobOutput;
    const NAME: &'static str = "search_glob";
    const DESCRIPTION: &'static str = "Glob-based path match. Sorting by name (default) or mtime (newest first). Stateless pagination via head_limit+offset.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move {
            tools
                .search_glob(
                    input.pattern,
                    input.path,
                    input.ignore,
                    input.include_hidden,
                    input.sort,
                    input.head_limit,
                    input.offset,
                )
                .await
        })
    }
}

// ============================================================================
// JustSearch Tool
// ============================================================================

/// Input for the just_search tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct JustSearchInput {
    /// Search query (substring match on name/docs)
    #[serde(default)]
    pub query: Option<String>,
    /// Directory filter (repo-relative or absolute)
    #[serde(default)]
    pub dir: Option<String>,
}

/// Tool for searching justfile recipes.
#[derive(Clone)]
pub struct JustSearchTool {
    tools: Arc<CodingAgentTools>,
}

impl JustSearchTool {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self { tools }
    }
}

impl Tool for JustSearchTool {
    type Input = JustSearchInput;
    type Output = just::SearchOutput;
    const NAME: &'static str = "just_search";
    const DESCRIPTION: &'static str = "Search justfile recipes by name or docs. Optional dir filter. Same params => next page. Page size: 10.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move { tools.just_search(input.query, input.dir).await })
    }
}

// ============================================================================
// JustExecute Tool
// ============================================================================

/// Input for the just_execute tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct JustExecuteInput {
    /// Recipe name (e.g., 'check', 'test', 'build')
    pub recipe: String,
    /// Directory containing the justfile (optional; defaults to root if recipe exists there)
    #[serde(default)]
    pub dir: Option<String>,
    /// Arguments keyed by parameter name; star params accept arrays
    #[serde(default)]
    pub args: Option<HashMap<String, serde_json::Value>>,
}

/// Tool for executing justfile recipes.
#[derive(Clone)]
pub struct JustExecuteTool {
    tools: Arc<CodingAgentTools>,
}

impl JustExecuteTool {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self { tools }
    }
}

impl Tool for JustExecuteTool {
    type Input = JustExecuteInput;
    type Output = just::ExecuteOutput;
    const NAME: &'static str = "just_execute";
    const DESCRIPTION: &'static str = "Execute a just recipe. Defaults to root justfile if no dir specified. Only disambiguate if recipe not in root.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let tools = self.tools.clone();
        Box::pin(async move {
            tools
                .just_execute(input.recipe, input.dir, input.args)
                .await
        })
    }
}

// ============================================================================
// Registry Builder
// ============================================================================

/// Build a ToolRegistry containing all coding_agent_tools.
pub fn build_registry(tools: Arc<CodingAgentTools>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<LsTool, ()>(LsTool::new(tools.clone()))
        .register::<SpawnAgentTool, ()>(SpawnAgentTool::new(tools.clone()))
        .register::<SearchGrepTool, ()>(SearchGrepTool::new(tools.clone()))
        .register::<SearchGlobTool, ()>(SearchGlobTool::new(tools.clone()))
        .register::<JustSearchTool, ()>(JustSearchTool::new(tools.clone()))
        .register::<JustExecuteTool, ()>(JustExecuteTool::new(tools))
        .finish()
}

// Error conversion removed - methods now return agentic_tools_core::ToolError directly
