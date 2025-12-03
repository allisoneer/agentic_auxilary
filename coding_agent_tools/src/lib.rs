pub mod pagination;
pub mod paths;
pub mod types;
pub mod walker;

use std::sync::Arc;
use types::{Depth, LsOutput, Show};
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
