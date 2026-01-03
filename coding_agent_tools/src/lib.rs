pub mod agent;
pub mod glob;
pub mod grep;
pub mod just;
pub mod pagination;
pub mod paths;
pub mod types;
pub mod walker;

use std::sync::Arc;
use types::{AgentOutput, Depth, GlobOutput, GrepOutput, LsOutput, OutputMode, Show, SortOrder};
use universal_tool_core::prelude::*;

/// Select the first non-empty (after trimming) text from result.result or result.content.
/// Prefers `result.result` over `result.content`, but rejects empty/whitespace-only strings.
fn pick_non_empty_text(result: &claudecode::types::Result) -> Option<String> {
    result
        .result
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .or_else(|| {
            result
                .content
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .cloned()
        })
}

#[derive(Clone)]
pub struct CodingAgentTools {
    /// Two-level pagination cache for MCP (persists across calls when Arc-wrapped)
    pager: Arc<pagination::PaginationCache>,
    /// Cache for parsed justfile recipes
    just_registry: Arc<just::JustRegistry>,
    /// Pagination cache for just search results
    just_pager: Arc<just::pager::PaginationCache>,
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
            just_registry: Arc::new(just::JustRegistry::new()),
            just_pager: Arc::new(just::pager::PaginationCache::new()),
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
        let abs_root = match paths::to_abs_string(&path_str) {
            Ok(s) => s,
            Err(msg) => return Err(ToolError::invalid_input(msg)),
        };
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
        description = "Spawn a Claude subagent for discovery or deep analysis. Returns a single text response; no side effects.

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
- Multiple spawn_agent calls can run in parallel for independent queries.",
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
        use claudecode::mcp::validate::{ValidateOptions, ensure_valid_mcp_config};
        use claudecode::types::{OutputFormat, PermissionMode};

        let agent_type = agent_type.unwrap_or_default();
        let location = location.unwrap_or_default();

        if query.trim().is_empty() {
            return Err(ToolError::invalid_input("Query cannot be empty"));
        }

        // Compose configuration
        let model = agent::model_for(agent_type);
        let system_prompt = agent::compose_prompt(agent_type, location);
        let enabled_tools = agent::enabled_tools_for(agent_type, location);

        // Split enabled tools into built-in vs MCP
        let (builtin_tools, _mcp_tools): (Vec<String>, Vec<String>) = enabled_tools
            .iter()
            .cloned()
            .partition(|t| !t.starts_with("mcp__"));

        // Compute MCP tools to disallow from our own server
        let disallowed_mcp = agent::disallowed_mcp_tools_for(&enabled_tools, location);

        // Build MCP config with enabled tools for CLI flag propagation
        let mcp_config = agent::build_mcp_config(location, &enabled_tools);

        // Validate MCP servers before launching (spawn, handshake, tools/list)
        let opts = ValidateOptions::default();
        ensure_valid_mcp_config(&mcp_config, &opts)
            .await
            .map_err(|e| {
                let mut details = String::new();
                for (name, err) in &e.errors {
                    details.push_str(&format!("  {}: {}\n", name, err));
                }
                ToolError::internal(format!(
                    "spawn_agent unavailable: MCP config validation failed.\n{}",
                    details
                ))
            })?;

        // Build session config
        let builder = SessionConfig::builder(query)
            .model(model)
            .output_format(OutputFormat::Text)
            .permission_mode(PermissionMode::DontAsk)
            .system_prompt(system_prompt)
            .tools(builtin_tools) // controls built-in tools in schema
            .allowed_tools(enabled_tools.clone()) // auto-approve enabled tools (built-in + MCP)
            .disallowed_tools(disallowed_mcp) // hide unwanted MCP tools from our server
            .mcp_config(mcp_config)
            .strict_mcp_config(true); // prevent inheritance of global MCP tools

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

        // Return plain text output (reject empty/whitespace-only strings)
        if let Some(text) = pick_non_empty_text(&result) {
            return Ok(AgentOutput::new(text));
        }

        Err(ToolError::internal(
            "Claude session produced no text output (empty or whitespace-only)",
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
        let abs_root = match paths::to_abs_string(&path_str) {
            Ok(s) => s,
            Err(msg) => return Err(ToolError::invalid_input(msg)),
        };
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
        let abs_root = match paths::to_abs_string(&path_str) {
            Ok(s) => s,
            Err(msg) => return Err(ToolError::invalid_input(msg)),
        };
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

    /// Search justfile recipes by name or docs.
    #[universal_tool(
        description = "Search justfile recipes by name or docs. Optional dir filter. Same params => next page. Page size: 10.",
        mcp(read_only = true, output = "text")
    )]
    pub async fn search(
        &self,
        #[universal_tool_param(description = "Search query (substring match on name/docs)")]
        query: Option<String>,
        #[universal_tool_param(description = "Directory filter (repo-relative or absolute)")]
        dir: Option<String>,
    ) -> Result<just::SearchOutput, ToolError> {
        just::ensure_just_available()
            .await
            .map_err(ToolError::internal)?;

        let repo_root = paths::to_abs_string(".").map_err(ToolError::internal)?;
        let q = query.unwrap_or_default();
        let dir_filter = dir
            .as_ref()
            .map(|d| paths::to_abs_string(d))
            .transpose()
            .map_err(ToolError::internal)?;

        self.just_pager.sweep_expired();
        let key = just::pager::make_key(dir_filter.as_deref().unwrap_or(&repo_root), &q);
        let qlock = self.just_pager.get_or_create(&key);

        // Check if we need to refresh - do this without holding lock across await
        let needs_refresh = {
            let st = qlock.state.lock().unwrap();
            st.is_empty() || st.is_expired()
        };

        // Fetch recipes if needed (outside lock)
        if needs_refresh {
            let all = self
                .just_registry
                .get_all_recipes(&repo_root)
                .await
                .map_err(ToolError::internal)?;

            let filtered: Vec<_> = all
                .into_iter()
                .filter(|(recipe_dir, r)| {
                    let dir_ok = dir_filter
                        .as_ref()
                        .map(|f| recipe_dir.starts_with(f))
                        .unwrap_or(true);
                    let visible = !r.is_private && !r.is_mcp_hidden;
                    let q_ok = q.is_empty()
                        || r.name
                            .to_ascii_lowercase()
                            .contains(&q.to_ascii_lowercase())
                        || r.doc
                            .as_ref()
                            .map(|d| d.to_ascii_lowercase().contains(&q.to_ascii_lowercase()))
                            .unwrap_or(false);
                    dir_ok && visible && q_ok
                })
                .map(|(d, r)| {
                    let params = r
                        .params
                        .iter()
                        .map(|p| {
                            if p.kind == just::parser::ParamKind::Star {
                                format!("{}*", p.name)
                            } else if p.has_default {
                                format!("{}?", p.name)
                            } else {
                                p.name.clone()
                            }
                        })
                        .collect();
                    just::SearchItem {
                        recipe: r.name,
                        dir: d,
                        doc: r.doc,
                        params,
                    }
                })
                .collect();

            // Reacquire lock to update state
            let mut st = qlock.state.lock().unwrap();
            st.reset(filtered);
        }

        // Paginate (separate lock acquisition)
        let (items, has_more) = {
            let mut st = qlock.state.lock().unwrap();
            let offset = st.next_offset;
            let end = (offset + just::pager::PAGE_SIZE).min(st.results.len());
            let page = st.results[offset..end].to_vec();
            st.next_offset = end;
            let has_more = end < st.results.len();
            (page, has_more)
        };

        if !has_more {
            self.just_pager.remove_if_same(&key, &qlock);
        }
        Ok(just::SearchOutput { items, has_more })
    }

    /// Execute a just recipe.
    #[universal_tool(
        description = "Execute a just recipe. If recipe exists in multiple justfiles, provide dir to disambiguate.",
        mcp(read_only = false, output = "text")
    )]
    pub async fn execute(
        &self,
        #[universal_tool_param(description = "Recipe name (e.g., 'check', 'test', 'build')")]
        recipe: String,
        #[universal_tool_param(
            description = "Directory containing the justfile (from search results)"
        )]
        dir: Option<String>,
        #[universal_tool_param(
            description = "Arguments keyed by parameter name; star params accept arrays"
        )]
        args: Option<std::collections::HashMap<String, serde_json::Value>>,
    ) -> Result<just::ExecuteOutput, ToolError> {
        just::ensure_just_available()
            .await
            .map_err(ToolError::internal)?;
        let repo_root = paths::to_abs_string(".").map_err(ToolError::internal)?;
        just::exec::execute_recipe(&self.just_registry, &recipe, dir, args, &repo_root)
            .await
            .map_err(ToolError::internal)
    }
}

use std::collections::HashSet;

// MCP server wrapper with optional tool allowlist
pub struct CodingAgentToolsServer {
    tools: Arc<CodingAgentTools>,
    /// None => expose all tools (backwards compatible);
    /// Some(set) => only expose tools whose names are in the set.
    allowlist: Option<HashSet<String>>,
}

impl CodingAgentToolsServer {
    pub fn new(tools: Arc<CodingAgentTools>) -> Self {
        Self {
            tools,
            allowlist: None,
        }
    }

    pub fn with_allowlist(
        tools: Arc<CodingAgentTools>,
        allowlist: Option<HashSet<String>>,
    ) -> Self {
        // Normalize empty set to None (treat as "all")
        let normalized = match allowlist {
            Some(set) if set.is_empty() => None,
            other => other,
        };
        Self {
            tools,
            allowlist: normalized,
        }
    }
}

// Manual ServerHandler implementation with tool filtering
impl universal_tool_core::mcp::ServerHandler for CodingAgentToolsServer {
    async fn initialize(
        &self,
        _params: universal_tool_core::mcp::InitializeRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::InitializeResult, universal_tool_core::mcp::McpError>
    {
        Ok(universal_tool_core::mcp::InitializeResult {
            server_info: universal_tool_core::mcp::Implementation {
                name: "CodingAgentToolsServer".to_string(),
                title: env!("CARGO_PKG_NAME").to_string().into(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                website_url: None,
                icons: None,
            },
            capabilities: universal_tool_core::mcp::ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        })
    }

    async fn list_tools(
        &self,
        _request: Option<universal_tool_core::mcp::PaginatedRequestParam>,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::ListToolsResult, universal_tool_core::mcp::McpError> {
        let all_defs = self.tools.get_mcp_tools();
        // Filter by allowlist if set
        let filtered_defs = match &self.allowlist {
            None => all_defs,
            Some(set) => all_defs
                .into_iter()
                .filter(|j| {
                    j.get("name")
                        .and_then(|v| v.as_str())
                        .map(|n| set.contains(n))
                        .unwrap_or(false)
                })
                .collect(),
        };
        let tools = universal_tool_core::mcp::convert_tool_definitions(filtered_defs);
        Ok(universal_tool_core::mcp::ListToolsResult::with_all_items(
            tools,
        ))
    }

    async fn call_tool(
        &self,
        request: universal_tool_core::mcp::CallToolRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::CallToolResult, universal_tool_core::mcp::McpError> {
        // Runtime check: deny calls to disallowed tools
        if let Some(set) = &self.allowlist
            && !set.contains(&*request.name)
        {
            return Ok(universal_tool_core::mcp::CallToolResult::error(vec![
                universal_tool_core::mcp::Content::text(format!(
                    "Tool '{}' not enabled on server",
                    request.name
                )),
            ]));
        }
        // Dispatch to underlying handler
        match self
            .tools
            .handle_mcp_call_mcp(
                &request.name,
                universal_tool_core::mcp::JsonValue::Object(request.arguments.unwrap_or_default()),
            )
            .await
        {
            Ok(result) => {
                Ok(universal_tool_core::mcp::IntoCallToolResult::into_call_tool_result(result))
            }
            Err(e) => Ok(universal_tool_core::mcp::CallToolResult::error(vec![
                universal_tool_core::mcp::Content::text(format!("Error: {}", e)),
            ])),
        }
    }

    // Default implementations for other required methods
    async fn ping(
        &self,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<(), universal_tool_core::mcp::McpError> {
        Ok(())
    }

    async fn complete(
        &self,
        _request: universal_tool_core::mcp::CompleteRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::CompleteResult, universal_tool_core::mcp::McpError> {
        Err(universal_tool_core::mcp::McpError::invalid_request(
            "Method not implemented",
            None,
        ))
    }

    async fn set_level(
        &self,
        _request: universal_tool_core::mcp::SetLevelRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<(), universal_tool_core::mcp::McpError> {
        Ok(())
    }

    async fn get_prompt(
        &self,
        _request: universal_tool_core::mcp::GetPromptRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::GetPromptResult, universal_tool_core::mcp::McpError> {
        Err(universal_tool_core::mcp::McpError::invalid_request(
            "Method not implemented",
            None,
        ))
    }

    async fn list_prompts(
        &self,
        _request: Option<universal_tool_core::mcp::PaginatedRequestParam>,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::ListPromptsResult, universal_tool_core::mcp::McpError>
    {
        Ok(universal_tool_core::mcp::ListPromptsResult {
            prompts: vec![],
            next_cursor: None,
        })
    }

    async fn list_resources(
        &self,
        _request: Option<universal_tool_core::mcp::PaginatedRequestParam>,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::ListResourcesResult, universal_tool_core::mcp::McpError>
    {
        Ok(universal_tool_core::mcp::ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<universal_tool_core::mcp::PaginatedRequestParam>,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<
        universal_tool_core::mcp::ListResourceTemplatesResult,
        universal_tool_core::mcp::McpError,
    > {
        Ok(universal_tool_core::mcp::ListResourceTemplatesResult {
            resource_templates: vec![],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        _request: universal_tool_core::mcp::ReadResourceRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<universal_tool_core::mcp::ReadResourceResult, universal_tool_core::mcp::McpError>
    {
        Err(universal_tool_core::mcp::McpError::invalid_request(
            "Method not implemented",
            None,
        ))
    }

    async fn subscribe(
        &self,
        _request: universal_tool_core::mcp::SubscribeRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<(), universal_tool_core::mcp::McpError> {
        Err(universal_tool_core::mcp::McpError::invalid_request(
            "Method not implemented",
            None,
        ))
    }

    async fn unsubscribe(
        &self,
        _request: universal_tool_core::mcp::UnsubscribeRequestParam,
        _context: universal_tool_core::mcp::RequestContext<universal_tool_core::mcp::RoleServer>,
    ) -> Result<(), universal_tool_core::mcp::McpError> {
        Err(universal_tool_core::mcp::McpError::invalid_request(
            "Method not implemented",
            None,
        ))
    }
}

// TODO(2): Consider adding allowlist/filtering support to universal_tool_core's implement_mcp_server!
// macro or ServerHandler trait. The manual impl above replaces the macro to add filtering, but
// this pattern could be generalized. Think about the best API design - perhaps:
// - implement_mcp_server!(MyServer, tools, allowlist: my_allowlist_field)
// - Or a separate trait/wrapper like FilteredServerHandler
// - Or a builder pattern for the macro-generated impl

#[cfg(test)]
mod spawn_agent_filter_tests {
    use super::*;
    use claudecode::types::Result as ClaudeResult;

    #[test]
    fn prefers_content_when_result_is_empty_string() {
        let r = ClaudeResult {
            result: Some("".into()),
            content: Some("ok".into()),
            ..Default::default()
        };
        assert_eq!(pick_non_empty_text(&r).as_deref(), Some("ok"));
    }

    #[test]
    fn returns_none_when_both_empty_or_whitespace() {
        let r1 = ClaudeResult {
            result: None,
            content: Some("".into()),
            ..Default::default()
        };
        assert_eq!(pick_non_empty_text(&r1), None);

        let r2 = ClaudeResult {
            result: None,
            content: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(pick_non_empty_text(&r2), None);

        let r3 = ClaudeResult {
            result: Some("   ".into()),
            content: None,
            ..Default::default()
        };
        assert_eq!(pick_non_empty_text(&r3), None);
    }

    #[test]
    fn returns_text_when_present_in_content() {
        let r = ClaudeResult {
            result: None,
            content: Some("text".into()),
            ..Default::default()
        };
        assert_eq!(pick_non_empty_text(&r).as_deref(), Some("text"));
    }

    #[test]
    fn respects_precedence_of_result_over_content() {
        let r = ClaudeResult {
            result: Some("  result text  ".into()),
            content: Some("other".into()),
            ..Default::default()
        };
        // Helper uses trim().is_empty() for emptiness check, but returns original string
        assert_eq!(pick_non_empty_text(&r).as_deref(), Some("  result text  "));
    }
}

#[cfg(test)]
mod server_allowlist_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn list_tools_filters_by_allowlist() {
        let tools = Arc::new(CodingAgentTools::new());
        let mut set = HashSet::new();
        set.insert("ls".to_string());
        set.insert("search_glob".to_string());
        let server = CodingAgentToolsServer::with_allowlist(tools.clone(), Some(set));

        let all_defs = server.tools.get_mcp_tools();
        // Filter using same logic as list_tools
        let filtered: Vec<_> = all_defs
            .into_iter()
            .filter(|j| {
                j.get("name")
                    .and_then(|v| v.as_str())
                    .map(|n| server.allowlist.as_ref().unwrap().contains(n))
                    .unwrap_or(false)
            })
            .collect();

        let names: Vec<_> = filtered
            .iter()
            .filter_map(|j| j.get("name").and_then(|v| v.as_str()))
            .collect();

        assert!(names.contains(&"ls"));
        assert!(names.contains(&"search_glob"));
        assert!(!names.contains(&"spawn_agent"));
        assert!(!names.contains(&"search_grep"));
    }

    #[test]
    fn allowlist_none_exposes_all() {
        let tools = Arc::new(CodingAgentTools::new());
        let server = CodingAgentToolsServer::with_allowlist(tools.clone(), None);

        let all_defs = server.tools.get_mcp_tools();
        assert_eq!(all_defs.len(), 6); // ls, spawn_agent, search_grep, search_glob, search, execute
    }

    #[test]
    fn empty_set_normalizes_to_none() {
        let tools = Arc::new(CodingAgentTools::new());
        let server = CodingAgentToolsServer::with_allowlist(tools, Some(HashSet::new()));
        assert!(server.allowlist.is_none());
    }

    #[test]
    fn new_constructor_has_no_allowlist() {
        let tools = Arc::new(CodingAgentTools::new());
        let server = CodingAgentToolsServer::new(tools);
        assert!(server.allowlist.is_none());
    }

    #[test]
    fn single_tool_allowlist() {
        let tools = Arc::new(CodingAgentTools::new());
        let mut set = HashSet::new();
        set.insert("spawn_agent".to_string());
        let server = CodingAgentToolsServer::with_allowlist(tools.clone(), Some(set));

        let all_defs = server.tools.get_mcp_tools();
        let filtered: Vec<_> = all_defs
            .into_iter()
            .filter(|j| {
                j.get("name")
                    .and_then(|v| v.as_str())
                    .map(|n| server.allowlist.as_ref().unwrap().contains(n))
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].get("name").and_then(|v| v.as_str()),
            Some("spawn_agent")
        );
    }
}
