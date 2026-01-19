pub mod agent;
pub mod glob;
pub mod grep;
pub mod just;
mod logging;
pub mod pagination;
pub mod paths;
pub mod tools;
pub mod types;
pub mod walker;

pub use tools::build_registry;

use agentic_tools_core::ToolError;
use std::sync::Arc;
use types::{AgentOutput, Depth, GlobOutput, GrepOutput, LsOutput, OutputMode, Show, SortOrder};

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

// Removed universal-tool-core macros; Tool impls live in tools.rs
impl CodingAgentTools {
    /// List files and directories (gitignore-aware)
    pub async fn ls(
        &self,
        path: Option<String>,
        depth: Option<Depth>,
        show: Option<Show>,
        ignore: Option<Vec<String>>,
        hidden: Option<bool>,
    ) -> Result<LsOutput, ToolError> {
        use std::path::Path;

        // Start logging context
        let log_ctx = logging::ToolLogCtx::start("cli_ls");
        let req_json = serde_json::json!({
            "path": &path,
            "depth": depth.map(|d| d.as_u8()),
            "show": show.map(|s| format!("{:?}", s).to_lowercase()),
            "ignore": &ignore,
            "hidden": hidden,
        });

        // Resolve path
        let path_str = path.unwrap_or_else(|| ".".into());
        let abs_root = match paths::to_abs_string(&path_str) {
            Ok(s) => s,
            Err(msg) => {
                log_ctx.finish(req_json, None, false, Some(msg.clone()), None, None, None);
                return Err(ToolError::InvalidInput(msg));
            }
        };
        let root_path = Path::new(&abs_root);

        // Validate root path exists and is a directory
        if !root_path.exists() {
            let error_msg = format!("Path does not exist: {}", abs_root);
            log_ctx.finish(
                req_json,
                None,
                false,
                Some(error_msg.clone()),
                None,
                None,
                None,
            );
            return Err(ToolError::InvalidInput(error_msg));
        }

        // Handle file path: return header with warning
        if root_path.is_file() {
            let output = LsOutput {
                root: abs_root,
                entries: vec![],
                has_more: false,
                warnings: vec![
                    "Path is a file, not a directory. Use the 'read' tool to view file contents."
                        .into(),
                ],
            };
            let summary = serde_json::json!({
                "entries": 0,
                "has_more": false,
                "is_file": true,
            });
            log_ctx.finish(req_json, None, true, None, Some(summary), None, None);
            return Ok(output);
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
                match walker::list(&cfg) {
                    Ok(result) => st.reset(result.entries, result.warnings, page_size),
                    Err(e) => {
                        drop(st);
                        log_ctx.finish(
                            req_json,
                            None,
                            false,
                            Some(e.to_string()),
                            None,
                            None,
                            None,
                        );
                        return Err(e);
                    }
                }
            }

            // Compute current page from cached results
            let offset = st.next_offset;
            let (page, has_more) = pagination::paginate_slice(&st.results, offset, st.page_size);

            // Advance offset for next call
            st.next_offset = st.next_offset.saturating_add(st.page_size);

            // Compute counts for truncation message
            let shown = (offset + page.len()).min(st.results.len());
            let total = st.results.len();

            (page, has_more, st.meta.clone(), shown, total)
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

        let output = LsOutput {
            root: abs_root,
            entries,
            has_more,
            warnings: all_warnings,
        };

        // Log success with summary
        let summary = serde_json::json!({
            "entries": output.entries.len(),
            "has_more": output.has_more,
            "shown": shown,
            "total": total,
        });
        log_ctx.finish(req_json, None, true, None, Some(summary), None, None);

        Ok(output)
    }

    /// Spawn an opinionated Claude subagent (locator | analyzer) in a specific location.
    pub async fn spawn_agent(
        &self,
        agent_type: Option<types::AgentType>,
        location: Option<types::AgentLocation>,
        query: String,
    ) -> Result<AgentOutput, ToolError> {
        use claudecode::client::Client;
        use claudecode::config::SessionConfig;
        use claudecode::mcp::validate::{ValidateOptions, ensure_valid_mcp_config};
        use claudecode::types::{OutputFormat, PermissionMode};

        // Start logging context
        let log_ctx = logging::ToolLogCtx::start("ask_agent");
        let agent_type = agent_type.unwrap_or_default();
        let location = location.unwrap_or_default();

        let req_json = serde_json::json!({
            "agent_type": format!("{:?}", agent_type).to_lowercase(),
            "location": format!("{:?}", location).to_lowercase(),
            "query": &query,
        });

        if query.trim().is_empty() {
            log_ctx.finish(
                req_json,
                None,
                false,
                Some("Query cannot be empty".into()),
                None,
                None,
                None,
            );
            return Err(ToolError::InvalidInput("Query cannot be empty".into()));
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
        if let Err(e) = ensure_valid_mcp_config(&mcp_config, &opts).await {
            let mut details = String::new();
            for (name, err) in &e.errors {
                details.push_str(&format!("  {}: {}\n", name, err));
            }
            let error_msg = format!(
                "spawn_agent unavailable: MCP config validation failed.\n{}",
                details
            );
            log_ctx.finish(
                req_json,
                None,
                false,
                Some(error_msg.clone()),
                None,
                Some(model.to_string()),
                None,
            );
            return Err(ToolError::Internal(error_msg.to_string()));
        }

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

        let config = match builder.build() {
            Ok(c) => c,
            Err(e) => {
                let error_msg = format!("Failed to build session config: {e}");
                log_ctx.finish(
                    req_json,
                    None,
                    false,
                    Some(error_msg.clone()),
                    None,
                    Some(model.to_string()),
                    None,
                );
                return Err(ToolError::Internal(error_msg.to_string()));
            }
        };

        // Ensure 'claude' binary exists
        let client = match Client::new().await {
            Ok(c) => c,
            Err(e) => {
                let error_msg = format!(
                    "Claude CLI not found or not runnable: {e}. Ensure 'claude' is installed and available in PATH."
                );
                log_ctx.finish(
                    req_json,
                    None,
                    false,
                    Some(error_msg.clone()),
                    None,
                    Some(model.to_string()),
                    None,
                );
                return Err(ToolError::Internal(error_msg.to_string()));
            }
        };

        let result = match client.launch_and_wait(config).await {
            Ok(r) => r,
            Err(e) => {
                let error_msg = format!("Failed to run Claude session: {e}");
                log_ctx.finish(
                    req_json,
                    None,
                    false,
                    Some(error_msg.clone()),
                    None,
                    Some(model.to_string()),
                    None,
                );
                return Err(ToolError::Internal(error_msg.to_string()));
            }
        };

        if result.is_error {
            let error_msg = result
                .error
                .clone()
                .unwrap_or_else(|| "Claude session returned an error".into());
            log_ctx.finish(
                req_json,
                None,
                false,
                Some(error_msg.clone()),
                None,
                Some(model.to_string()),
                None,
            );
            return Err(ToolError::Internal(error_msg.to_string()));
        }

        // Return plain text output (reject empty/whitespace-only strings)
        if let Some(text) = pick_non_empty_text(&result) {
            // Write markdown response file and capture timestamp for consistent logging
            let (response_file, completed_at) = log_ctx
                .write_markdown_response(&text)
                .map(|(f, ts)| (Some(f), Some(ts)))
                .unwrap_or((None, None));
            log_ctx.finish(
                req_json,
                response_file,
                true,
                None,
                None,
                Some(model.to_string()),
                completed_at,
            );
            return Ok(AgentOutput::new(text));
        }

        let error_msg = "Claude session produced no text output (empty or whitespace-only)";
        log_ctx.finish(
            req_json,
            None,
            false,
            Some(error_msg.into()),
            None,
            Some(model.to_string()),
            None,
        );
        Err(ToolError::Internal(error_msg.to_string()))
    }

    /// Search the codebase using a regex pattern (gitignore-aware).
    pub async fn search_grep(
        &self,
        pattern: String,
        path: Option<String>,
        mode: Option<OutputMode>,
        globs: Option<Vec<String>>,
        ignore: Option<Vec<String>>,
        include_hidden: Option<bool>,
        case_insensitive: Option<bool>,
        multiline: Option<bool>,
        line_numbers: Option<bool>,
        context: Option<u32>,
        context_before: Option<u32>,
        context_after: Option<u32>,
        include_binary: Option<bool>,
        head_limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<GrepOutput, ToolError> {
        // Start logging context
        let log_ctx = logging::ToolLogCtx::start("cli_grep");
        let req_json = serde_json::json!({
            "pattern": &pattern,
            "path": &path,
            "mode": mode.map(|m| format!("{:?}", m).to_lowercase()),
            "globs": &globs,
            "ignore": &ignore,
            "include_hidden": include_hidden,
            "case_insensitive": case_insensitive,
            "multiline": multiline,
            "line_numbers": line_numbers,
            "context": context,
            "context_before": context_before,
            "context_after": context_after,
            "include_binary": include_binary,
            "head_limit": head_limit,
            "offset": offset,
        });

        let path_str = path.unwrap_or_else(|| ".".into());
        let abs_root = match paths::to_abs_string(&path_str) {
            Ok(s) => s,
            Err(msg) => {
                log_ctx.finish(req_json, None, false, Some(msg.clone()), None, None, None);
                return Err(ToolError::InvalidInput(msg));
            }
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

        match grep::run(cfg) {
            Ok(output) => {
                let summary = serde_json::json!({
                    "lines": output.lines.len(),
                    "mode": format!("{:?}", output.mode).to_lowercase(),
                    "has_more": output.has_more,
                });
                log_ctx.finish(req_json, None, true, None, Some(summary), None, None);
                Ok(output)
            }
            Err(e) => {
                log_ctx.finish(req_json, None, false, Some(e.to_string()), None, None, None);
                Err(e)
            }
        }
    }

    /// Match files/directories by glob pattern (gitignore-aware).
    pub async fn search_glob(
        &self,
        pattern: String,
        path: Option<String>,
        ignore: Option<Vec<String>>,
        include_hidden: Option<bool>,
        sort: Option<SortOrder>,
        head_limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<GlobOutput, ToolError> {
        // Start logging context
        let log_ctx = logging::ToolLogCtx::start("cli_glob");
        let req_json = serde_json::json!({
            "pattern": &pattern,
            "path": &path,
            "ignore": &ignore,
            "include_hidden": include_hidden,
            "sort": sort.map(|s| format!("{:?}", s).to_lowercase()),
            "head_limit": head_limit,
            "offset": offset,
        });

        let path_str = path.unwrap_or_else(|| ".".into());
        let abs_root = match paths::to_abs_string(&path_str) {
            Ok(s) => s,
            Err(msg) => {
                log_ctx.finish(req_json, None, false, Some(msg.clone()), None, None, None);
                return Err(ToolError::InvalidInput(msg));
            }
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

        match glob::run(cfg) {
            Ok(output) => {
                let summary = serde_json::json!({
                    "entries": output.entries.len(),
                    "has_more": output.has_more,
                });
                log_ctx.finish(req_json, None, true, None, Some(summary), None, None);
                Ok(output)
            }
            Err(e) => {
                log_ctx.finish(req_json, None, false, Some(e.to_string()), None, None, None);
                Err(e)
            }
        }
    }

    /// Search justfile recipes by name or docs.
    pub async fn just_search(
        &self,
        query: Option<String>,
        dir: Option<String>,
    ) -> Result<just::SearchOutput, ToolError> {
        // Start logging context
        let log_ctx = logging::ToolLogCtx::start("cli_just_search");
        let req_json = serde_json::json!({
            "query": &query,
            "dir": &dir,
        });

        if let Err(e) = just::ensure_just_available().await {
            let error_msg = e.to_string();
            log_ctx.finish(
                req_json,
                None,
                false,
                Some(error_msg.clone()),
                None,
                None,
                None,
            );
            return Err(ToolError::Internal(error_msg.to_string()));
        }

        let repo_root = match paths::to_abs_string(".") {
            Ok(r) => r,
            Err(e) => {
                log_ctx.finish(req_json, None, false, Some(e.clone()), None, None, None);
                return Err(ToolError::Internal(e));
            }
        };
        let q = query.unwrap_or_default();
        let dir_filter = match dir.as_ref().map(|d| paths::to_abs_string(d)).transpose() {
            Ok(f) => f,
            Err(e) => {
                log_ctx.finish(req_json, None, false, Some(e.clone()), None, None, None);
                return Err(ToolError::Internal(e));
            }
        };

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
            let all = match self.just_registry.get_all_recipes(&repo_root).await {
                Ok(r) => r,
                Err(e) => {
                    let error_msg = e.to_string();
                    log_ctx.finish(
                        req_json,
                        None,
                        false,
                        Some(error_msg.clone()),
                        None,
                        None,
                        None,
                    );
                    return Err(ToolError::Internal(error_msg.to_string()));
                }
            };

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

        let output = just::SearchOutput { items, has_more };

        // Log success with summary
        let summary = serde_json::json!({
            "items": output.items.len(),
            "has_more": output.has_more,
        });
        log_ctx.finish(req_json, None, true, None, Some(summary), None, None);

        Ok(output)
    }

    /// Execute a just recipe.
    pub async fn just_execute(
        &self,
        recipe: String,
        dir: Option<String>,
        args: Option<std::collections::HashMap<String, serde_json::Value>>,
    ) -> Result<just::ExecuteOutput, ToolError> {
        // Start logging context
        let log_ctx = logging::ToolLogCtx::start("cli_just_execute");
        let req_json = serde_json::json!({
            "recipe": &recipe,
            "dir": &dir,
            "args": &args,
        });

        if let Err(e) = just::ensure_just_available().await {
            let error_msg = e.to_string();
            log_ctx.finish(
                req_json,
                None,
                false,
                Some(error_msg.clone()),
                None,
                None,
                None,
            );
            return Err(ToolError::Internal(error_msg.to_string()));
        }

        let repo_root = match paths::to_abs_string(".") {
            Ok(r) => r,
            Err(e) => {
                log_ctx.finish(req_json, None, false, Some(e.clone()), None, None, None);
                return Err(ToolError::Internal(e));
            }
        };

        match just::exec::execute_recipe(&self.just_registry, &recipe, dir, args, &repo_root).await
        {
            Ok(output) => {
                let summary = serde_json::json!({
                    "exit_code": output.exit_code,
                    "stdout_lines": output.stdout.lines().count(),
                    "stderr_lines": output.stderr.lines().count(),
                });
                log_ctx.finish(req_json, None, true, None, Some(summary), None, None);
                Ok(output)
            }
            Err(e) => {
                let error_msg = e.to_string();
                log_ctx.finish(
                    req_json,
                    None,
                    false,
                    Some(error_msg.clone()),
                    None,
                    None,
                    None,
                );
                Err(ToolError::Internal(error_msg.to_string()))
            }
        }
    }
}

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
