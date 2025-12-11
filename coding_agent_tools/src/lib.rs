pub mod agent;
pub mod glob;
pub mod grep;
pub mod pagination;
pub mod paths;
pub mod types;
pub mod walker;

use std::sync::Arc;
use types::{AgentOutput, Depth, GlobOutput, GrepOutput, LsOutput, OutputMode, Show, SortOrder};
use universal_tool_core::prelude::*;

#[derive(Clone)]
pub struct CodingAgentTools {
    /// Two-level pagination cache for MCP (persists across calls when Arc-wrapped)
    pager: Arc<pagination::PaginationCache>,
}

impl Default for CodingAgentTools {
    fn default() -> Self {
        Self::new()
    }
}

impl CodingAgentTools {
    pub fn new() -> Self {
        Self {
            pager: Arc::new(pagination::PaginationCache::new()),
        }
    }
}

#[universal_tool_router(mcp(name = "coding-agent-tools", version = "0.1.0"))]
impl CodingAgentTools {
    /// List files and directories (gitignore-aware)
    #[universal_tool(
        description = "List files and directories. Depth: 0=header only, 1=children (default), 2-10=tree. Filter with show='files'|'dirs'|'all'. Gitignore-aware. For shallow queries, call with same params again for next page.",
        mcp(read_only = true, output = "text")
    )]
    pub async fn ls(
        &self,
        #[universal_tool_param(description = "Directory path (absolute or relative to cwd)")]
        path: Option<String>,
        #[universal_tool_param(
            description = "Traversal depth: 0=header, 1=children (default), 2-10=tree"
        )]
        depth: Option<Depth>,
        #[universal_tool_param(description = "Filter: 'all' (default), 'files', or 'dirs'")]
        show: Option<Show>,
        #[universal_tool_param(description = "Additional glob patterns to ignore")] ignore: Option<
            Vec<String>,
        >,
        #[universal_tool_param(description = "Include hidden files (default: false)")]
        hidden: Option<bool>,
    ) -> Result<LsOutput, ToolError> {
        use std::path::Path;

        // Resolve path
        let path_str = path.unwrap_or_else(|| ".".into());
        let abs_root = paths::to_abs_string(&path_str);
        let root_path = Path::new(&abs_root);

        // Validate root path exists and is a directory
        if !root_path.exists() {
            return Err(ToolError::invalid_input(format!(
                "Path does not exist: {}",
                abs_root
            )));
        }

        // Handle file path: return header with warning
        if root_path.is_file() {
            return Ok(LsOutput {
                root: abs_root,
                entries: vec![],
                has_more: false,
                warnings: vec![
                    "Path is a file, not a directory. Use the 'read' tool to view file contents."
                        .into(),
                ],
            });
        }

        // Configure walker
        let depth_val = depth.map(|d| d.as_u8()).unwrap_or(1);
        let show_val = show.unwrap_or_default();
        let user_ignores = ignore.unwrap_or_default();
        let include_hidden = hidden.unwrap_or(false);

        let cfg = walker::WalkConfig {
            root: root_path,
            depth: depth_val,
            show: show_val,
            user_ignores: &user_ignores,
            include_hidden,
        };

        // Sweep expired cache entries opportunistically
        self.pager.sweep_expired();

        // Determine pagination params
        let page_size = pagination::page_size_for(show_val, depth_val);
        let query_key = pagination::make_key(
            &abs_root,
            depth_val,
            show_val,
            include_hidden,
            &user_ignores,
        );

        // Acquire per-query lock (level 2), serialize same-param calls
        let qlock = self.pager.get_or_create(&query_key);
        let (entries, has_more, warnings, shown, total) = {
            let mut st = qlock.state.lock().unwrap();

            // Fill cache if empty or expired
            if st.is_empty() || st.is_expired() {
                let result = walker::list(&cfg)?;
                st.reset(result.entries, result.warnings, page_size);
            }

            // Compute current page from cached results
            let offset = st.next_offset;
            let (page, has_more) = pagination::paginate_slice(&st.results, offset, st.page_size);

            // Advance offset for next call
            st.next_offset = st.next_offset.saturating_add(st.page_size);

            // Compute counts for truncation message
            let shown = (offset + page.len()).min(st.results.len());
            let total = st.results.len();

            (page, has_more, st.warnings.clone(), shown, total)
        };

        // Prepare enhanced truncation info using sentinel
        let mut all_warnings = warnings;
        if has_more {
            let encoded = types::encode_truncation_info(shown, total, page_size);
            all_warnings.insert(0, encoded);
        }

        // If this was the last page, proactively remove cache entry
        if !has_more {
            self.pager.remove_if_same(&query_key, &qlock);
        }

        Ok(LsOutput {
            root: abs_root,
            entries,
            has_more,
            warnings: all_warnings,
        })
    }

    /// Spawn an opinionated Claude subagent (locator | analyzer) in a specific location.
    #[universal_tool(
        description = "Spawn an opinionated Claude subagent to perform discovery or deep analysis across codebase, thoughts, references, or the web. Returns a single text response; no side effects.",
        mcp(read_only = true, output = "text")
    )]
    pub async fn spawn_agent(
        &self,
        #[universal_tool_param(
            description = "Agent type: 'locator' (fast discovery, haiku) or 'analyzer' (deep analysis, sonnet). Default: locator"
        )]
        agent_type: Option<types::AgentType>,
        #[universal_tool_param(
            description = "Location: 'codebase'|'thoughts'|'references'|'web'. Default: codebase"
        )]
        location: Option<types::AgentLocation>,
        #[universal_tool_param(
            description = "Task to perform; plain language question/instructions for the subagent"
        )]
        query: String,
    ) -> Result<AgentOutput, ToolError> {
        use claudecode::client::Client;
        use claudecode::config::SessionConfig;
        use claudecode::types::{OutputFormat, PermissionMode};

        let agent_type = agent_type.unwrap_or_default();
        let location = location.unwrap_or_default();

        if query.trim().is_empty() {
            return Err(ToolError::invalid_input("Query cannot be empty"));
        }

        // Early validation for required binaries
        agent::require_binaries_for_location(location)?;

        // Compose configuration
        let model = agent::model_for(agent_type);
        let system_prompt = agent::compose_prompt(agent_type, location);
        let allowed_tools = agent::allowed_tools_for(agent_type, location);

        // Working directory resolution (may be None for web)
        let working_dir = agent::resolve_working_dir(location)?;

        // Build MCP config
        let mcp_config = agent::build_mcp_config(location);

        // Build session config
        let mut builder = SessionConfig::builder(query)
            .model(model)
            .output_format(OutputFormat::Text)
            .permission_mode(PermissionMode::DontAsk)
            .system_prompt(system_prompt)
            .allowed_tools(allowed_tools)
            .mcp_config(mcp_config);

        if let Some(dir) = working_dir {
            builder = builder.working_dir(dir);
        }

        let config = builder
            .build()
            .map_err(|e| ToolError::internal(format!("Failed to build session config: {e}")))?;

        // Ensure 'claude' binary exists
        let client = Client::new().await.map_err(|e| {
            ToolError::internal(format!(
                "Claude CLI not found or not runnable: {e}. Ensure 'claude' is installed and available in PATH."
            ))
        })?;

        let result = client
            .launch_and_wait(config)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to run Claude session: {e}")))?;

        if result.is_error {
            return Err(ToolError::internal(
                result
                    .error
                    .unwrap_or_else(|| "Claude session returned an error".into()),
            ));
        }

        // Return plain text output
        if let Some(text) = result.result.or(result.content) {
            return Ok(AgentOutput::new(text));
        }

        Err(ToolError::internal(
            "Claude session finished without text content",
        ))
    }

    /// Search the codebase using a regex pattern (gitignore-aware).
    #[universal_tool(
        description = "Regex-based search. Modes: files (default), content, count. Stateless pagination via head_limit+offset.",
        mcp(read_only = true, output = "text")
    )]
    pub async fn search_grep(
        &self,
        #[universal_tool_param(description = "Regex pattern to search for")] pattern: String,
        #[universal_tool_param(description = "Directory path (absolute or relative to cwd)")]
        path: Option<String>,
        #[universal_tool_param(
            description = "Output mode: 'files' (default), 'content', or 'count'"
        )]
        mode: Option<OutputMode>,
        #[universal_tool_param(description = "Include-only glob patterns (files to consider)")]
        globs: Option<Vec<String>>,
        #[universal_tool_param(description = "Additional glob patterns to ignore (exclude)")]
        ignore: Option<Vec<String>>,
        #[universal_tool_param(description = "Include hidden files (default: false)")]
        include_hidden: Option<bool>,
        #[universal_tool_param(description = "Case-insensitive matching (default: false)")]
        case_insensitive: Option<bool>,
        #[universal_tool_param(
            description = "Allow '.' to match newlines; patterns may span lines (default: false)"
        )]
        multiline: Option<bool>,
        #[universal_tool_param(description = "Show line numbers in content mode (default: true)")]
        line_numbers: Option<bool>,
        #[universal_tool_param(
            description = "Context lines before and after matches (overridden by context_before/after if provided)"
        )]
        context: Option<u32>,
        #[universal_tool_param(description = "Context lines before match")] context_before: Option<
            u32,
        >,
        #[universal_tool_param(description = "Context lines after match")] context_after: Option<
            u32,
        >,
        #[universal_tool_param(description = "Search binary files as text (default: false)")]
        include_binary: Option<bool>,
        #[universal_tool_param(
            description = "Max results to return (default: 200, capped at 1000)"
        )]
        head_limit: Option<usize>,
        #[universal_tool_param(description = "Skip the first N results (default: 0)")]
        offset: Option<usize>,
    ) -> Result<GrepOutput, ToolError> {
        let path_str = path.unwrap_or_else(|| ".".into());
        let abs_root = paths::to_abs_string(&path_str);
        let cfg = grep::GrepConfig {
            root: abs_root,
            pattern,
            mode: mode.unwrap_or_default(),
            include_globs: globs.unwrap_or_default(),
            ignore_globs: ignore.unwrap_or_default(),
            include_hidden: include_hidden.unwrap_or(false),
            case_insensitive: case_insensitive.unwrap_or(false),
            multiline: multiline.unwrap_or(false),
            line_numbers: line_numbers.unwrap_or(true),
            context,
            context_before,
            context_after,
            include_binary: include_binary.unwrap_or(false),
            head_limit: head_limit.unwrap_or(200),
            offset: offset.unwrap_or(0),
        };
        grep::run(cfg)
    }

    /// Match files/directories by glob pattern (gitignore-aware).
    #[universal_tool(
        description = "Glob-based path match. Sorting by name (default) or mtime (newest first). Stateless pagination via head_limit+offset.",
        mcp(read_only = true, output = "text")
    )]
    pub async fn search_glob(
        &self,
        #[universal_tool_param(description = "Glob pattern to match against (e.g., '**/*.rs')")]
        pattern: String,
        #[universal_tool_param(description = "Directory path (absolute or relative to cwd)")]
        path: Option<String>,
        #[universal_tool_param(description = "Additional glob patterns to ignore (exclude)")]
        ignore: Option<Vec<String>>,
        #[universal_tool_param(description = "Include hidden files (default: false)")]
        include_hidden: Option<bool>,
        #[universal_tool_param(
            description = "Sort order: 'name' (default) or 'mtime' (newest first)"
        )]
        sort: Option<SortOrder>,
        #[universal_tool_param(
            description = "Max results to return (default: 500, capped at 1000)"
        )]
        head_limit: Option<usize>,
        #[universal_tool_param(description = "Skip the first N results (default: 0)")]
        offset: Option<usize>,
    ) -> Result<GlobOutput, ToolError> {
        let path_str = path.unwrap_or_else(|| ".".into());
        let abs_root = paths::to_abs_string(&path_str);
        let cfg = glob::GlobConfig {
            root: abs_root,
            pattern,
            ignore_globs: ignore.unwrap_or_default(),
            include_hidden: include_hidden.unwrap_or(false),
            sort: sort.unwrap_or_default(),
            head_limit: head_limit.unwrap_or(500),
            offset: offset.unwrap_or(0),
        };
        glob::run(cfg)
    }
}

// MCP server wrapper
pub struct CodingAgentToolsServer {
    tools: Arc<CodingAgentTools>,
}

impl CodingAgentToolsServer {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self { tools }
    }
}

universal_tool_core::implement_mcp_server!(CodingAgentToolsServer, tools);
