pub mod pagination;
pub mod paths;
pub mod types;
pub mod walker;

use std::sync::Arc;
use tokio::sync::Mutex;
use types::{Depth, LsOutput, Show};
use universal_tool_core::prelude::*;

#[derive(Clone)]
pub struct CodingAgentTools {
    /// Pagination state for MCP (persists across calls when Arc-wrapped)
    pager: Arc<Mutex<Option<pagination::LastQuery>>>,
}

impl Default for CodingAgentTools {
    fn default() -> Self {
        Self::new()
    }
}

impl CodingAgentTools {
    pub fn new() -> Self {
        Self {
            pager: Arc::new(Mutex::new(None)),
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

        let result = walker::list(&cfg)?;

        // Determine pagination
        let page_size = pagination::page_size_for(show_val, depth_val);
        let query_key = pagination::make_key(
            &abs_root,
            depth_val,
            show_val,
            include_hidden,
            &user_ignores,
        );

        // Get offset from pagination state
        let offset = {
            let mut pager = self.pager.lock().await;

            let same_and_fresh = matches!(
                pager.as_ref(),
                Some(last) if last.key == query_key && last.is_fresh()
            );

            if same_and_fresh {
                // Same query, advance to next page
                pager.as_mut().unwrap().advance()
            } else {
                // New query or expired - create state and advance to first page offset
                pager
                    .insert(pagination::LastQuery::new(query_key, page_size))
                    .advance()
            }
        };

        // Apply pagination
        let (paginated_entries, has_more) = pagination::paginate(result.entries, offset, page_size);

        Ok(LsOutput {
            root: abs_root,
            entries: paginated_entries,
            has_more,
            warnings: result.warnings,
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
