use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use universal_tool_core::mcp::{McpFormatter, ServiceExt};
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

// Helper for human-readable sizes
fn human_size(bytes: u64) -> String {
    match bytes {
        0 => "0 B".into(),
        1..=1023 => format!("{} B", bytes),
        1024..=1048575 => format!("{:.1} KB", (bytes as f64) / 1024.0),
        _ => format!("{:.1} MB", (bytes as f64) / (1024.0 * 1024.0)),
    }
}

// New result types for MCP text formatting

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteDocumentOk {
    pub path: String,
    pub bytes_written: u64,
}

impl McpFormatter for WriteDocumentOk {
    fn mcp_format_text(&self) -> String {
        format!(
            "✓ Created {}\n  Size: {}",
            self.path,
            human_size(self.bytes_written)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActiveDocuments {
    pub base: String,
    pub files: Vec<DocumentInfo>,
}

impl McpFormatter for ActiveDocuments {
    fn mcp_format_text(&self) -> String {
        if self.files.is_empty() {
            return format!(
                "Active base: {}\nFiles (relative to base):\n<none>",
                self.base
            );
        }
        let mut out = format!("Active base: {}\nFiles (relative to base):", self.base);
        for f in &self.files {
            let rel = f
                .path
                .strip_prefix(&format!("{}/", self.base))
                .unwrap_or(&f.path);
            let ts = match chrono::DateTime::parse_from_rfc3339(&f.modified) {
                Ok(dt) => dt
                    .with_timezone(&chrono::Utc)
                    .format("%Y-%m-%d %H:%M UTC")
                    .to_string(),
                Err(_) => f.modified.clone(),
            };
            out.push_str(&format!("\n{} @ {}", rel, ts));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReferenceItem {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReferencesList {
    pub base: String,
    pub entries: Vec<ReferenceItem>,
}

impl McpFormatter for ReferencesList {
    fn mcp_format_text(&self) -> String {
        if self.entries.is_empty() {
            return format!("References base: {}\n<none>", self.base);
        }
        let mut out = format!("References base: {}", self.base);
        for e in &self.entries {
            let rel = e
                .path
                .strip_prefix(&format!("{}/", self.base))
                .unwrap_or(&e.path);
            match &e.description {
                Some(desc) if !desc.trim().is_empty() => {
                    out.push_str(&format!("\n{} — {}", rel, desc));
                }
                _ => {
                    out.push_str(&format!("\n{}", rel));
                }
            }
        }
        out
    }
}

// Tool implementation

#[derive(Clone, Default)]
pub struct ThoughtsMcpTools;

#[universal_tool_router(mcp(name = "thoughts_tool", version = "0.3.0"))]
impl ThoughtsMcpTools {
    /// Write markdown to active work directory (research/plans/artifacts)
    #[universal_tool(
        description = "Write markdown to the active work directory",
        mcp(destructive = false, output = "text")
    )]
    pub async fn write_document(
        &self,
        doc_type: DocumentType,
        filename: String,
        content: String,
    ) -> Result<WriteDocumentOk, ToolError> {
        // Validate filename
        validate_simple_filename(&filename).map_err(|e| ToolError::invalid_input(e.to_string()))?;

        // Ensure active work exists
        let aw =
            ensure_active_work().map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;

        // Determine target path
        let dir = doc_type.subdir(&aw);
        let target = dir.join(&filename);

        let bytes_written = content.len() as u64;

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
        Ok(WriteDocumentOk {
            path: repo_rel,
            bytes_written,
        })
    }

    /// List files in current active work directory
    #[universal_tool(
        description = "List files in the current active work directory",
        mcp(read_only = true, idempotent = true, output = "text")
    )]
    pub async fn list_active_documents(
        &self,
        subdir: Option<DocumentType>,
    ) -> Result<ActiveDocuments, ToolError> {
        let aw =
            ensure_active_work().map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;

        let base = format!("thoughts/active/{}", aw.dir_name);

        // Determine which subdirs to scan
        let sets: Vec<(String, std::path::PathBuf)> = match subdir {
            Some(d) => vec![(d.subdir_name().to_string(), d.subdir(&aw).clone())],
            None => vec![
                ("research".to_string(), aw.research.clone()),
                ("plans".to_string(), aw.plans.clone()),
                ("artifacts".to_string(), aw.artifacts.clone()),
            ],
        };

        let mut files = Vec::new();
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
                    files.push(DocumentInfo {
                        path: format!("{}/{}/{}", base, name, file_name),
                        doc_type: name.clone(),
                        size: meta.len(),
                        modified: modified.to_rfc3339(),
                    });
                }
            }
        }

        Ok(ActiveDocuments { base, files })
    }

    /// List reference repository directory paths
    #[universal_tool(
        description = "List reference repository directory paths (references/org/repo)",
        mcp(read_only = true, idempotent = true, output = "text")
    )]
    pub async fn list_references(&self) -> Result<ReferencesList, ToolError> {
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

        let base = ds.mount_dirs.references.clone();
        let mut entries = Vec::new();

        // Phase 3: ds.references is still Vec<String>; description: None
        for url in &ds.references {
            let path = match extract_org_repo_from_url(url) {
                Ok((org, repo)) => format!("{}/{}", org, repo),
                Err(_) => url.clone(),
            };
            entries.push(ReferenceItem {
                path: format!("{}/{}", base, path),
                description: None,
            });
        }

        Ok(ReferencesList { base, entries })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_size_formatting() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1), "1 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn test_write_document_ok_format() {
        let ok = WriteDocumentOk {
            path: "thoughts/active/feat/research/a.md".into(),
            bytes_written: 2048,
        };
        let text = ok.mcp_format_text();
        assert!(text.contains("2.0 KB"));
        assert!(text.contains("✓ Created"));
        assert!(text.contains("thoughts/active/feat/research/a.md"));
    }

    #[test]
    fn test_active_documents_empty() {
        let docs = ActiveDocuments {
            base: "thoughts/active/x".into(),
            files: vec![],
        };
        let s = docs.mcp_format_text();
        assert!(s.contains("<none>"));
        assert!(s.contains("thoughts/active/x"));
    }

    #[test]
    fn test_active_documents_with_files() {
        let docs = ActiveDocuments {
            base: "thoughts/active/feature".into(),
            files: vec![DocumentInfo {
                path: "thoughts/active/feature/research/test.md".into(),
                doc_type: "research".into(),
                size: 1024,
                modified: "2025-10-15T12:00:00Z".into(),
            }],
        };
        let text = docs.mcp_format_text();
        assert!(text.contains("research/test.md"));
        assert!(text.contains("2025-10-15 12:00 UTC"));
    }

    #[test]
    fn test_references_list_empty() {
        let refs = ReferencesList {
            base: "references".into(),
            entries: vec![],
        };
        let s = refs.mcp_format_text();
        assert!(s.contains("<none>"));
        assert!(s.contains("references"));
    }

    #[test]
    fn test_references_list_without_descriptions() {
        let refs = ReferencesList {
            base: "references".into(),
            entries: vec![
                ReferenceItem {
                    path: "references/org/repo1".into(),
                    description: None,
                },
                ReferenceItem {
                    path: "references/org/repo2".into(),
                    description: None,
                },
            ],
        };
        let text = refs.mcp_format_text();
        assert!(text.contains("org/repo1"));
        assert!(text.contains("org/repo2"));
        assert!(!text.contains("—")); // No description separator
    }

    #[test]
    fn test_references_list_with_descriptions() {
        let refs = ReferencesList {
            base: "references".into(),
            entries: vec![
                ReferenceItem {
                    path: "references/org/repo1".into(),
                    description: Some("First repo".into()),
                },
                ReferenceItem {
                    path: "references/org/repo2".into(),
                    description: Some("Second repo".into()),
                },
            ],
        };
        let text = refs.mcp_format_text();
        assert!(text.contains("org/repo1 — First repo"));
        assert!(text.contains("org/repo2 — Second repo"));
    }
}
