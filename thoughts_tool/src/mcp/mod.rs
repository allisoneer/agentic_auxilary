use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use universal_tool_core::mcp::{McpFormatter, ServiceExt};
use universal_tool_core::prelude::*;

mod templates;

use crate::config::validation::{canonical_reference_key, validate_reference_url_https_only};
use crate::config::{
    ReferenceEntry, ReferenceMount, RepoConfigManager, RepoMappingManager,
    extract_org_repo_from_url,
};
use crate::git::utils::get_control_repo_root;
use crate::mount::auto_mount::update_active_mounts;
use crate::mount::get_mount_manager;
use crate::platform::detect_platform;
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

    // TODO(1): API asymmetry - subdir_name() returns plural ("plans", "artifacts") but
    // serde serialization uses singular ("plan", "artifact"). This causes list_active_documents
    // to return doc_type values that don't match input parameter values, breaking client filtering.
    // When fixing: also update prompts in coding_agent_tools/src/agent/prompts.rs and
    // .opencode/command/*.md back to singular forms.
    // See: thoughts/active/.../research/doc_type_asymmetry_analysis.md
    fn subdir_name(&self) -> &'static str {
        match self {
            DocumentType::Research => "research",
            DocumentType::Plan => "plans",
            DocumentType::Artifact => "artifacts",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TemplateType {
    Research,
    Plan,
    Requirements,
    PrDescription,
}

impl TemplateType {
    pub fn label(&self) -> &'static str {
        match self {
            TemplateType::Research => "research",
            TemplateType::Plan => "plan",
            TemplateType::Requirements => "requirements",
            TemplateType::PrDescription => "pr_description",
        }
    }
    pub fn content(&self) -> &'static str {
        match self {
            TemplateType::Research => templates::RESEARCH_TEMPLATE_MD,
            TemplateType::Plan => templates::PLAN_TEMPLATE_MD,
            TemplateType::Requirements => templates::REQUIREMENTS_TEMPLATE_MD,
            TemplateType::PrDescription => templates::PR_DESCRIPTION_TEMPLATE_MD,
        }
    }
    pub fn guidance(&self) -> &'static str {
        match self {
            TemplateType::Research => templates::RESEARCH_GUIDANCE,
            TemplateType::Plan => templates::PLAN_GUIDANCE,
            TemplateType::Requirements => templates::REQUIREMENTS_GUIDANCE,
            TemplateType::PrDescription => templates::PR_DESCRIPTION_GUIDANCE,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddReferenceOk {
    pub url: String,
    pub org: String,
    pub repo: String,
    pub mount_path: String,
    pub mount_target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_path: Option<String>,
    pub already_existed: bool,
    pub config_updated: bool,
    pub cloned: bool,
    pub mounted: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl McpFormatter for AddReferenceOk {
    fn mcp_format_text(&self) -> String {
        let mut out = String::new();
        if self.already_existed {
            out.push_str("✓ Reference already exists (idempotent)\n");
        } else {
            out.push_str("✓ Added reference\n");
        }
        out.push_str(&format!(
            "  URL: {}\n  Org/Repo: {}/{}\n  Mount: {}\n  Target: {}",
            self.url, self.org, self.repo, self.mount_path, self.mount_target
        ));
        if let Some(mp) = &self.mapping_path {
            out.push_str(&format!("\n  Mapping: {}", mp));
        } else {
            out.push_str("\n  Mapping: <none>");
        }
        out.push_str(&format!(
            "\n  Config updated: {}\n  Cloned: {}\n  Mounted: {}",
            self.config_updated, self.cloned, self.mounted
        ));
        if !self.warnings.is_empty() {
            out.push_str("\nWarnings:");
            for w in &self.warnings {
                out.push_str(&format!("\n- {}", w));
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemplateResponse {
    pub template_type: TemplateType,
}

impl McpFormatter for TemplateResponse {
    fn mcp_format_text(&self) -> String {
        let ty = self.template_type.label();
        let content = self.template_type.content();
        let guidance = self.template_type.guidance();
        format!(
            "Here is the {} template:\n\n```markdown\n{}\n```\n\n{}",
            ty, content, guidance
        )
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

        // Phase 4: ds.references is now Vec<ReferenceMount> with optional descriptions
        for rm in &ds.references {
            let path = match extract_org_repo_from_url(&rm.remote) {
                Ok((org, repo)) => format!("{}/{}", org, repo),
                Err(_) => rm.remote.clone(),
            };
            entries.push(ReferenceItem {
                path: format!("{}/{}", base, path),
                description: rm.description.clone(),
            });
        }

        Ok(ReferencesList { base, entries })
    }

    /// Add a GitHub repository as a reference and ensure it is cloned and mounted.
    ///
    /// Input must be an HTTPS GitHub URL: https://github.com/org/repo or
    /// https://github.com/org/repo.git. Also accepts generic https://*.git clone URLs.
    /// SSH URLs (git@…) are rejected. The operation is idempotent and safe to retry;
    /// first-time clones may take time. Returns details about config changes, clone
    /// location, and mount status.
    #[universal_tool(
        description = "Add a GitHub repository as a reference and ensure it is cloned and mounted. Input must be an HTTPS GitHub URL (https://github.com/org/repo or .git) or generic https://*.git clone URL. SSH URLs (git@…) are rejected. Idempotent and safe to retry; first-time clones may take time.",
        mcp(destructive = false, idempotent = true, output = "text")
    )]
    pub async fn add_reference(
        &self,
        #[universal_tool_param(
            description = "HTTPS GitHub URL (https://github.com/org/repo) or generic https://*.git clone URL"
        )]
        url: String,
        #[universal_tool_param(
            description = "Optional description for why this reference was added"
        )]
        description: Option<String>,
    ) -> Result<AddReferenceOk, ToolError> {
        let input_url = url.trim().to_string();

        // Validate URL per MCP HTTPS-only rules
        validate_reference_url_https_only(&input_url)
            .map_err(|e| ToolError::invalid_input(e.to_string()))?;

        // Parse org/repo; safe after validation
        let (org, repo) = extract_org_repo_from_url(&input_url)
            .map_err(|e| ToolError::invalid_input(e.to_string()))?;

        // Resolve repo root and config manager
        let repo_root = get_control_repo_root(
            &std::env::current_dir()
                .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?,
        )
        .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;

        let mgr = RepoConfigManager::new(repo_root.clone());
        let mut cfg = mgr
            .ensure_v2_default()
            .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;

        // Build existing canonical keys set for duplicate detection
        let mut existing_keys = std::collections::HashSet::new();
        for e in &cfg.references {
            let existing_url = match e {
                ReferenceEntry::Simple(s) => s.as_str(),
                ReferenceEntry::WithMetadata(rm) => rm.remote.as_str(),
            };
            if let Ok(k) = canonical_reference_key(existing_url) {
                existing_keys.insert(k);
            }
        }
        let this_key = canonical_reference_key(&input_url)
            .map_err(|e| ToolError::invalid_input(e.to_string()))?;
        let already_existed = existing_keys.contains(&this_key);

        // Compute paths for response
        let ds = mgr
            .load_desired_state()
            .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?
            .ok_or_else(|| {
                ToolError::new(ErrorCode::NotFound, "No repository configuration found")
            })?;
        let mount_path = format!("{}/{}/{}", ds.mount_dirs.references, org, repo);
        let mount_target = repo_root
            .join(".thoughts-data")
            .join(&mount_path)
            .to_string_lossy()
            .to_string();

        // Capture pre-sync mapping status
        let repo_mapping = RepoMappingManager::new()
            .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
        let pre_mapping = repo_mapping
            .resolve_url(&input_url)
            .ok()
            .flatten()
            .map(|p| p.to_string_lossy().to_string());

        // Update config if new
        let mut config_updated = false;
        let mut warnings: Vec<String> = Vec::new();
        if !already_existed {
            if let Some(desc) = description.clone() {
                cfg.references
                    .push(ReferenceEntry::WithMetadata(ReferenceMount {
                        remote: input_url.clone(),
                        description: if desc.trim().is_empty() {
                            None
                        } else {
                            Some(desc)
                        },
                    }));
            } else {
                cfg.references
                    .push(ReferenceEntry::Simple(input_url.clone()));
            }

            let ws = mgr
                .save_v2_validated(&cfg)
                .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
            warnings.extend(ws);
            config_updated = true;
        } else if description.is_some() {
            warnings.push(
                "Reference already exists; description was not updated (use CLI to modify metadata)"
                    .to_string(),
            );
        }

        // Always attempt to sync clone+mount (best-effort, no rollback)
        if let Err(e) = update_active_mounts().await {
            warnings.push(format!("Mount synchronization encountered an error: {}", e));
        }

        // Post-sync mapping status to infer cloning
        let repo_mapping_post = RepoMappingManager::new()
            .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
        let post_mapping = repo_mapping_post
            .resolve_url(&input_url)
            .ok()
            .flatten()
            .map(|p| p.to_string_lossy().to_string());
        let cloned = pre_mapping.is_none() && post_mapping.is_some();

        // Determine mounted by listing active mounts
        let platform =
            detect_platform().map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
        let mount_manager = get_mount_manager(&platform)
            .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
        let active = mount_manager
            .list_mounts()
            .await
            .map_err(|e| ToolError::new(ErrorCode::IoError, e.to_string()))?;
        let target_path = std::path::PathBuf::from(&mount_target);
        let target_canon = std::fs::canonicalize(&target_path).unwrap_or(target_path.clone());
        let mut mounted = false;
        for mi in active {
            let canon = std::fs::canonicalize(&mi.target).unwrap_or(mi.target.clone());
            if canon == target_canon {
                mounted = true;
                break;
            }
        }

        // Additional warnings for visibility
        if post_mapping.is_none() {
            warnings.push(
                "Repository was not cloned or mapped. It may be private or network unavailable. \
                 You can retry or run 'thoughts references sync' via CLI."
                    .to_string(),
            );
        }
        if !mounted {
            warnings.push(
                "Mount is not active. You can retry or run 'thoughts mount update' via CLI."
                    .to_string(),
            );
        }

        Ok(AddReferenceOk {
            url: input_url,
            org,
            repo,
            mount_path,
            mount_target,
            mapping_path: post_mapping,
            already_existed,
            config_updated,
            cloned,
            mounted,
            warnings,
        })
    }

    /// Get a compile-time embedded document template with usage guidance.
    #[universal_tool(
        description = "Return a compile-time embedded template (research, plan, requirements, pr_description) with usage guidance",
        mcp(read_only = true, idempotent = true, output = "text")
    )]
    pub async fn get_template(
        &self,
        #[universal_tool_param(
            description = "Which template to fetch (research, plan, requirements, pr_description)"
        )]
        template: TemplateType,
    ) -> Result<TemplateResponse, ToolError> {
        Ok(TemplateResponse {
            template_type: template,
        })
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

    #[test]
    fn test_add_reference_ok_format() {
        let ok = AddReferenceOk {
            url: "https://github.com/org/repo".into(),
            org: "org".into(),
            repo: "repo".into(),
            mount_path: "references/org/repo".into(),
            mount_target: "/abs/.thoughts-data/references/org/repo".into(),
            mapping_path: Some("/home/user/.thoughts/clones/repo".into()),
            already_existed: false,
            config_updated: true,
            cloned: true,
            mounted: true,
            warnings: vec!["note".into()],
        };
        let s = ok.mcp_format_text();
        assert!(s.contains("✓ Added reference"));
        assert!(s.contains("Org/Repo: org/repo"));
        assert!(s.contains("Cloned: true"));
        assert!(s.contains("Mounted: true"));
        assert!(s.contains("Warnings:\n- note"));
    }

    #[test]
    fn test_add_reference_ok_format_already_existed() {
        let ok = AddReferenceOk {
            url: "https://github.com/org/repo".into(),
            org: "org".into(),
            repo: "repo".into(),
            mount_path: "references/org/repo".into(),
            mount_target: "/abs/.thoughts-data/references/org/repo".into(),
            mapping_path: Some("/home/user/.thoughts/clones/repo".into()),
            already_existed: true,
            config_updated: false,
            cloned: false,
            mounted: true,
            warnings: vec![],
        };
        let s = ok.mcp_format_text();
        assert!(s.contains("✓ Reference already exists (idempotent)"));
        assert!(s.contains("Config updated: false"));
        assert!(!s.contains("Warnings:"));
    }

    #[test]
    fn test_add_reference_ok_format_no_mapping() {
        let ok = AddReferenceOk {
            url: "https://github.com/org/repo".into(),
            org: "org".into(),
            repo: "repo".into(),
            mount_path: "references/org/repo".into(),
            mount_target: "/abs/.thoughts-data/references/org/repo".into(),
            mapping_path: None,
            already_existed: false,
            config_updated: true,
            cloned: false,
            mounted: false,
            warnings: vec!["Clone failed".into()],
        };
        let s = ok.mcp_format_text();
        assert!(s.contains("Mapping: <none>"));
        assert!(s.contains("Mounted: false"));
        assert!(s.contains("- Clone failed"));
    }

    #[test]
    fn test_template_response_format_research() {
        let resp = TemplateResponse {
            template_type: TemplateType::Research,
        };
        let s = resp.mcp_format_text();
        assert!(s.starts_with("Here is the research template:"));
        assert!(s.contains("```markdown"));
        // spot-check content from the research template
        assert!(s.contains("# Research: [Topic]"));
        // research guidance presence
        assert!(s.contains("Before you write your research document"));
    }

    #[test]
    fn test_template_variants_non_empty() {
        let all = [
            TemplateType::Research,
            TemplateType::Plan,
            TemplateType::Requirements,
            TemplateType::PrDescription,
        ];
        for t in all {
            assert!(
                !t.content().trim().is_empty(),
                "Embedded content unexpectedly empty for {:?}",
                t
            );
            assert!(
                !t.label().trim().is_empty(),
                "Label unexpectedly empty for {:?}",
                t
            );
        }
    }
}
