use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use universal_tool_core::mcp::ServiceExt;
use universal_tool_core::prelude::*;

use crate::config::RepoConfigManager;
use crate::config::extract_org_repo_from_url;
use crate::utils::validation::validate_simple_filename;
use crate::workspace::{ActiveWork, ensure_active_work};

// Type definitions

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Research,
    Plan,
    Artifact,
}

impl DocumentType {
    fn subdir<'a>(&self, aw: &'a ActiveWork) -> &'a std::path::PathBuf {
        match self {
            DocumentType::Research => &aw.research,
            DocumentType::Plan => &aw.plans,
            DocumentType::Artifact => &aw.artifacts,
        }
    }

    fn subdir_name(&self) -> &'static str {
        match self {
            DocumentType::Research => "research",
            DocumentType::Plan => "plans",
            DocumentType::Artifact => "artifacts",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentInfo {
    pub path: String,
    pub doc_type: String,
    pub size: u64,
    pub modified: String,
}

// Tool implementation

#[derive(Clone, Default)]
pub struct ThoughtsMcpTools;

#[universal_tool_router(mcp(name = "thoughts_tool", version = "0.3.0"))]
impl ThoughtsMcpTools {
    /// Write markdown to active work directory (research/plans/artifacts)
    #[universal_tool(
        description = "Write markdown to the active work directory",
        mcp(destructive = false)
    )]
    pub async fn write_document(
        &self,
        doc_type: DocumentType,
        filename: String,
        content: String,
    ) -> Result<String, ToolError> {
        // Validate filename
        validate_simple_filename(&filename).map_err(|e| ToolError::invalid_input(e.to_string()))?;

        // Ensure active work exists
        let aw =
            ensure_active_work().map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;

        // Determine target path
        let dir = doc_type.subdir(&aw);
        let target = dir.join(&filename);

        // Atomic write
        let af =
            atomicwrites::AtomicFile::new(&target, atomicwrites::OverwriteBehavior::AllowOverwrite);
        af.write(|f| std::io::Write::write_all(f, content.as_bytes()))
            .map_err(|e| {
                ToolError::new(ErrorCode::IoError, format!("Failed to write file: {}", e))
            })?;

        // Return repo-relative path
        let repo_rel = format!(
            "thoughts/active/{}/{}/{}",
            aw.dir_name,
            doc_type.subdir_name(),
            filename
        );
        Ok(repo_rel)
    }

    /// List files in current active work directory
    #[universal_tool(
        description = "List files in the current active work directory",
        mcp(read_only = true, idempotent = true)
    )]
    pub async fn list_active_documents(
        &self,
        subdir: Option<DocumentType>,
    ) -> Result<Vec<DocumentInfo>, ToolError> {
        let aw =
            ensure_active_work().map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;

        // Determine which subdirs to scan
        let sets: Vec<(String, std::path::PathBuf)> = match subdir {
            Some(d) => vec![(d.subdir_name().to_string(), d.subdir(&aw).clone())],
            None => vec![
                ("research".to_string(), aw.research.clone()),
                ("plans".to_string(), aw.plans.clone()),
                ("artifacts".to_string(), aw.artifacts.clone()),
            ],
        };

        let mut out = Vec::new();
        for (name, dir) in sets {
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir).map_err(|e| {
                ToolError::new(
                    ErrorCode::IoError,
                    format!("Failed to read dir {}: {}", dir.display(), e),
                )
            })? {
                let entry = entry.map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
                let meta = entry
                    .metadata()
                    .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
                if meta.is_file() {
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|t| chrono::DateTime::<chrono::Utc>::from(t).into())
                        .unwrap_or_else(chrono::Utc::now);
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    out.push(DocumentInfo {
                        path: format!("thoughts/active/{}/{}/{}", aw.dir_name, name, file_name),
                        doc_type: name.clone(),
                        size: meta.len(),
                        modified: modified.to_rfc3339(),
                    });
                }
            }
        }

        Ok(out)
    }

    /// List reference repository directory paths
    #[universal_tool(
        description = "List reference repository directory paths (references/org/repo)",
        mcp(read_only = true, idempotent = true)
    )]
    pub async fn list_references(&self) -> Result<Vec<String>, ToolError> {
        let control_root = crate::git::utils::get_control_repo_root(
            &std::env::current_dir()
                .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?,
        )
        .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
        let mgr = RepoConfigManager::new(control_root);
        let ds = mgr
            .load_desired_state()
            .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?
            .ok_or_else(|| {
                ToolError::new(
                    universal_tool_core::error::ErrorCode::NotFound,
                    "No repository configuration found",
                )
            })?;

        let mut out = Vec::new();
        for url in &ds.references {
            match extract_org_repo_from_url(url) {
                Ok((org, repo)) => {
                    out.push(format!("{}/{}/{}", ds.mount_dirs.references, org, repo));
                }
                Err(_) => {
                    // Best-effort fallback for unparseable URLs
                    out.push(format!("{}/{}", ds.mount_dirs.references, url));
                }
            }
        }
        Ok(out)
    }
}

// MCP server wrapper
pub struct ThoughtsMcpServer {
    tools: std::sync::Arc<ThoughtsMcpTools>,
}
universal_tool_core::implement_mcp_server!(ThoughtsMcpServer, tools);

/// Serve MCP over stdio (called from main)
pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let server = ThoughtsMcpServer {
        tools: std::sync::Arc::new(ThoughtsMcpTools),
    };
    let transport = universal_tool_core::mcp::stdio();
    let svc = server.serve(transport).await?;
    svc.waiting().await?;
    Ok(())
}
