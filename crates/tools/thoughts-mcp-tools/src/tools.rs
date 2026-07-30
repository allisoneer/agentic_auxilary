//! Tool wrappers for `thoughts_tool` using agentic-tools-core.
//!
//! Each tool wraps the corresponding functionality from the `thoughts_tool`
//! library with logging identical to the MCP implementation.

use agentic_config::types::ThoughtsConfig;
use agentic_logging::CallTimer;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;
use std::path::PathBuf;

use thoughts_tool::config::RepoConfigManager;
use thoughts_tool::config::extract_org_repo_from_url;
use thoughts_tool::documents::ActiveDocuments;
use thoughts_tool::documents::DocumentType;
use thoughts_tool::documents::WriteDocumentOk;
use thoughts_tool::documents::list_documents;
use thoughts_tool::documents::write_document;
use thoughts_tool::git::ref_key::encode_ref_key;
use thoughts_tool::git::utils::get_control_repo_root;
use thoughts_tool::mcp::AddReferenceOk;
use thoughts_tool::mcp::ReferenceItem;
use thoughts_tool::mcp::ReferencesList;
use thoughts_tool::mcp::RepoRefsList;
use thoughts_tool::mcp::TemplateResponse;
use thoughts_tool::mcp::TemplateType;
use thoughts_tool::mcp::add_reference_impl_adapter;
use thoughts_tool::mcp::get_repo_refs_impl_adapter;
use thoughts_tool::mount::MountSpace;
use thoughts_tool::utils::logging::log_tool_call;

use crate::readiness::ThoughtsMcpReadinessGate;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ThoughtsReadInput {
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone)]
pub struct ReadDocumentTool {
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

#[derive(Clone)]
pub struct ReadReferenceTool {
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

impl Tool for ReadDocumentTool {
    type Input = ThoughtsReadInput;
    type Output = String;
    const NAME: &'static str = "thoughts_read_document";
    const DESCRIPTION: &'static str =
        "Read a bounded text file or directory inside the active Thoughts work directory.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let readiness = self.readiness.clone();
        Box::pin(async move {
            ensure_ready(&readiness).await?;
            let cwd =
                std::env::current_dir().map_err(|error| ToolError::Internal(error.to_string()))?;
            let base = thoughts_tool::workspace::resolve_active_work_base_read_only(&cwd)
                .map_err(|error| ToolError::Internal(error.to_string()))?;
            read_from_base(&base, &input)
        })
    }
}

impl Tool for ReadReferenceTool {
    type Input = ThoughtsReadInput;
    type Output = String;
    const NAME: &'static str = "thoughts_read_reference";
    const DESCRIPTION: &'static str =
        "Read a bounded text file or directory inside the configured References mount.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let readiness = self.readiness.clone();
        Box::pin(async move {
            ensure_ready(&readiness).await?;
            let base = references_base()?;
            read_from_base(&base, &input)
        })
    }
}

fn references_base() -> Result<PathBuf, ToolError> {
    let cwd = std::env::current_dir().map_err(|error| ToolError::Internal(error.to_string()))?;
    let base = thoughts_tool::workspace::resolve_references_base_read_only(&cwd)
        .map_err(|error| ToolError::Internal(error.to_string()))?;
    std::fs::canonicalize(base)
        .map_err(|error| ToolError::Internal(format!("Failed to resolve References base: {error}")))
}

fn read_from_base(base: &Path, input: &ThoughtsReadInput) -> Result<String, ToolError> {
    if input.file_path.trim().is_empty() {
        return Err(ToolError::InvalidInput("filePath is required.".to_string()));
    }
    let base = std::fs::canonicalize(base)
        .map_err(|error| ToolError::Internal(format!("Failed to resolve read base: {error}")))?;
    let requested = PathBuf::from(&input.file_path);
    let joined = if requested.is_absolute() {
        requested
    } else {
        base.join(requested)
    };
    let canonical = std::fs::canonicalize(&joined).map_err(|error| {
        let message = format!("Failed to resolve `{}`: {error}", input.file_path);
        if error.kind() == std::io::ErrorKind::NotFound {
            ToolError::NotFound(message)
        } else {
            ToolError::InvalidInput(message)
        }
    })?;
    if !canonical.starts_with(&base) {
        return Err(ToolError::InvalidInput(format!(
            "`{}` resolves outside the configured read base.",
            input.file_path
        )));
    }
    let display = canonical
        .strip_prefix(&base)
        .map_err(|error| ToolError::Internal(error.to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    workspace_tools::tools::render_bounded_path(
        &canonical,
        if display.is_empty() { "." } else { &display },
        input.offset,
        input.limit,
    )
}

/// Map `anyhow::Error` to `agentic_tools_core::ToolError`.
///
/// Uses string pattern matching to categorize errors appropriately.
fn map_anyhow_to_tool_error(e: &anyhow::Error) -> ToolError {
    let msg = e.to_string();
    let lc = msg.to_lowercase();
    if lc.contains("permission") || lc.contains("401") || lc.contains("403") {
        ToolError::Permission(msg)
    } else if lc.contains("not found") || lc.contains("404") {
        ToolError::NotFound(msg)
    } else if lc.contains("invalid") || lc.contains("bad request") {
        ToolError::InvalidInput(msg)
    } else if lc.contains("timeout") || lc.contains("network") {
        ToolError::External(msg)
    } else {
        ToolError::Internal(msg)
    }
}

async fn ensure_ready(readiness: &ThoughtsMcpReadinessGate) -> Result<(), ToolError> {
    readiness
        .ensure_ready()
        .await
        .map_err(|error| ToolError::Internal(format!("{error:#}")))
}

async fn ensure_ready_and_log_failure(
    readiness: &ThoughtsMcpReadinessGate,
    timer: &CallTimer,
    tool_name: &'static str,
    req_json: &serde_json::Value,
) -> Result<(), ToolError> {
    if let Err(error) = ensure_ready(readiness).await {
        log_tool_call(
            timer,
            tool_name,
            req_json.clone(),
            false,
            Some(error.to_string()),
            None,
        );
        return Err(error);
    }

    Ok(())
}

// ============================================================================
// WriteDocument Tool
// ============================================================================

/// Input for the `write_document` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WriteDocumentInput {
    pub doc_type: DocumentType,
    /// Filename for the document.
    pub filename: String,
    /// Content to write to the document.
    pub content: String,
}

/// Tool for writing documents to the active work directory.
#[derive(Clone)]
pub struct WriteDocumentTool {
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

impl Tool for WriteDocumentTool {
    type Input = WriteDocumentInput;
    type Output = WriteDocumentOk;
    const NAME: &'static str = "thoughts_write_document";
    const DESCRIPTION: &'static str = "Write markdown to the active work directory";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let readiness = self.readiness.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "doc_type": input.doc_type.singular_label(),
                "filename": &input.filename,
            });

            ensure_ready_and_log_failure(&readiness, &timer, "thoughts_write_document", &req_json)
                .await?;

            let result = write_document(&input.doc_type, &input.filename, &input.content);

            match &result {
                Ok(ok) => {
                    let summary = serde_json::json!({
                        "path": &ok.path,
                        "bytes_written": ok.bytes_written,
                    });
                    log_tool_call(
                        &timer,
                        "thoughts_write_document",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "thoughts_write_document",
                        req_json,
                        false,
                        Some(e.to_string()),
                        None,
                    );
                }
            }

            result.map_err(|e| ToolError::Internal(e.to_string()))
        })
    }
}

// ============================================================================
// ListActiveDocuments Tool
// ============================================================================

/// Input for the `list_active_documents` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListActiveDocumentsInput {
    /// Optional subdirectory filter by document type.
    #[serde(default)]
    pub subdir: Option<DocumentType>,
}

/// Tool for listing files in the active work directory.
#[derive(Clone)]
pub struct ListActiveDocumentsTool {
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

impl Tool for ListActiveDocumentsTool {
    type Input = ListActiveDocumentsInput;
    type Output = ActiveDocuments;
    const NAME: &'static str = "thoughts_list_documents";
    const DESCRIPTION: &'static str = "List files in the current active work directory";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let readiness = self.readiness.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "subdir": input.subdir.as_ref().map(|d| format!("{d:?}").to_lowercase()),
            });

            ensure_ready_and_log_failure(&readiness, &timer, "thoughts_list_documents", &req_json)
                .await?;

            let result = list_documents(input.subdir.as_ref());

            match &result {
                Ok(docs) => {
                    let summary = serde_json::json!({
                        "base": &docs.base,
                        "files_count": docs.files.len(),
                    });
                    log_tool_call(
                        &timer,
                        "thoughts_list_documents",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "thoughts_list_documents",
                        req_json,
                        false,
                        Some(e.to_string()),
                        None,
                    );
                }
            }

            result.map_err(|e| ToolError::Internal(e.to_string()))
        })
    }
}

// ============================================================================
// ListReferences Tool
// ============================================================================

/// Input for the `list_references` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct ListReferencesInput {}

/// Tool for listing reference repository directory paths.
#[derive(Clone)]
pub struct ListReferencesTool {
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

impl Tool for ListReferencesTool {
    type Input = ListReferencesInput;
    type Output = ReferencesList;
    const NAME: &'static str = "thoughts_list_references";
    const DESCRIPTION: &'static str = "List reference repository directory paths (references/org/repo or references/org/repo@ref_key)";

    fn call(
        &self,
        _input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let readiness = self.readiness.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({});

            ensure_ready_and_log_failure(&readiness, &timer, "thoughts_list_references", &req_json)
                .await?;

            let result = (|| -> Result<ReferencesList, ToolError> {
                let control_root = get_control_repo_root(
                    &std::env::current_dir().map_err(|e| ToolError::Internal(e.to_string()))?,
                )
                .map_err(|e| ToolError::Internal(e.to_string()))?;

                let mgr = RepoConfigManager::new(control_root);
                let ds = mgr
                    .load_desired_state()
                    .map_err(|e| ToolError::Internal(e.to_string()))?
                    .ok_or_else(|| {
                        ToolError::NotFound("No repository configuration found".into())
                    })?;

                let base = ds.mount_dirs.references.clone();
                let mut entries = Vec::new();

                for rm in &ds.references {
                    let path = match extract_org_repo_from_url(&rm.remote) {
                        Ok((org_path, repo)) => MountSpace::Reference {
                            org_path,
                            repo,
                            ref_key: rm
                                .ref_name
                                .as_deref()
                                .map(encode_ref_key)
                                .transpose()
                                .map_err(|e| ToolError::Internal(e.to_string()))?,
                        }
                        .relative_path(&ds.mount_dirs),
                        Err(_) => rm.remote.clone(),
                    };
                    entries.push(ReferenceItem {
                        path,
                        description: rm.description.clone(),
                    });
                }

                Ok(ReferencesList { base, entries })
            })();

            match &result {
                Ok(refs) => {
                    let summary = serde_json::json!({
                        "base": &refs.base,
                        "entries_count": refs.entries.len(),
                    });
                    log_tool_call(
                        &timer,
                        "thoughts_list_references",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "thoughts_list_references",
                        req_json,
                        false,
                        Some(e.to_string()),
                        None,
                    );
                }
            }

            result
        })
    }
}

// ============================================================================
// GetRepoRefs Tool
// ============================================================================

/// Input for the `get_repo_refs` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GetRepoRefsInput {
    /// HTTPS GitHub URL (<https://github.com/org/repo>) or generic https://*.git clone URL
    pub url: String,
    /// Maximum refs to return (1-200, default 100)
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Tool for listing remote refs without cloning the repository.
#[derive(Clone)]
pub struct GetRepoRefsTool {
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

impl Tool for GetRepoRefsTool {
    type Input = GetRepoRefsInput;
    type Output = RepoRefsList;
    const NAME: &'static str = "thoughts_get_repo_refs";
    const DESCRIPTION: &'static str =
        "List remote branches, tags, and full ref names for a repository without cloning it";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let readiness = self.readiness.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "url": &input.url,
                "limit": input.limit,
            });

            ensure_ready_and_log_failure(&readiness, &timer, "thoughts_get_repo_refs", &req_json)
                .await?;

            let result = get_repo_refs_impl_adapter(input.url, input.limit)
                .await
                .map_err(|e| map_anyhow_to_tool_error(&e));

            match &result {
                Ok(ok) => {
                    let summary = serde_json::json!({
                        "total": ok.total,
                        "returned": ok.entries.len(),
                        "truncated": ok.truncated,
                    });
                    log_tool_call(
                        &timer,
                        "thoughts_get_repo_refs",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "thoughts_get_repo_refs",
                        req_json,
                        false,
                        Some(e.to_string()),
                        None,
                    );
                }
            }

            result
        })
    }
}

// ============================================================================
// AddReference Tool
// ============================================================================

/// Input for the `add_reference` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AddReferenceInput {
    /// HTTPS GitHub URL (<https://github.com/org/repo>) or generic https://*.git clone URL
    pub url: String,
    /// Optional full git ref name to clone, which must start with refs/heads/
    /// or refs/tags/ (for example refs/heads/main).
    /// Shorthand values like "main" are rejected.
    /// Note: refs/remotes/* is a local remote-tracking namespace and is rejected for new inputs.
    #[serde(rename = "ref", default)]
    pub ref_name: Option<String>,
    /// Optional description for why this reference was added
    #[serde(default)]
    pub description: Option<String>,
}

/// Tool for adding a GitHub repository as a reference.
#[derive(Clone)]
pub struct AddReferenceTool {
    pub thoughts: ThoughtsConfig,
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

impl Tool for AddReferenceTool {
    type Input = AddReferenceInput;
    type Output = AddReferenceOk;
    const NAME: &'static str = "thoughts_add_reference";
    const DESCRIPTION: &'static str = "Add a GitHub repository as a reference and ensure it is cloned and mounted. Input must be an HTTPS GitHub URL (https://github.com/org/repo or .git) or generic https://*.git clone URL. Optional ref selects a full ref name (for example refs/heads/main). SSH URLs (git@\u{2026}) are rejected. Idempotent and safe to retry; first-time clones may take time.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let thoughts = self.thoughts.clone();
        let readiness = self.readiness.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "url": &input.url,
                "ref": &input.ref_name,
                "description": &input.description,
                "configured_timeout_secs": thoughts.add_reference_timeout_secs,
            });

            ensure_ready_and_log_failure(&readiness, &timer, "thoughts_add_reference", &req_json)
                .await?;

            // Delegate to the shared adapter function
            let result = add_reference_impl_adapter(
                input.url,
                input.description,
                input.ref_name,
                thoughts.add_reference_timeout_secs,
            )
            .await
            .map_err(|e| map_anyhow_to_tool_error(&e));

            match &result {
                Ok(ok) => {
                    let summary = serde_json::json!({
                        "ref": &ok.ref_name,
                        "org": &ok.org,
                        "repo": &ok.repo,
                        "already_existed": ok.already_existed,
                        "config_updated": ok.config_updated,
                        "cloned": ok.cloned,
                        "mounted": ok.mounted,
                    });
                    log_tool_call(
                        &timer,
                        "thoughts_add_reference",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "thoughts_add_reference",
                        req_json,
                        false,
                        Some(e.to_string()),
                        None,
                    );
                }
            }

            result
        })
    }
}

// ============================================================================
// GetTemplate Tool
// ============================================================================

/// Input for the `get_template` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GetTemplateInput {
    /// Which template to fetch (research, plan, requirements, `pr_description`)
    pub template: TemplateType,
}

/// Tool for retrieving compile-time embedded templates.
#[derive(Clone)]
pub struct GetTemplateTool {
    pub(crate) readiness: ThoughtsMcpReadinessGate,
}

impl Tool for GetTemplateTool {
    type Input = GetTemplateInput;
    type Output = TemplateResponse;
    const NAME: &'static str = "thoughts_get_template";
    const DESCRIPTION: &'static str = "Return a compile-time embedded template (research, plan, requirements, pr_description) with usage guidance";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let readiness = self.readiness.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "template": input.template.label(),
            });

            ensure_ready_and_log_failure(&readiness, &timer, "thoughts_get_template", &req_json)
                .await?;

            let result = TemplateResponse {
                template_type: input.template,
            };

            let summary = serde_json::json!({
                "template_type": result.template_type.label(),
            });
            log_tool_call(
                &timer,
                "thoughts_get_template",
                req_json,
                true,
                None,
                Some(summary),
            );

            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_tools_core::Tool;
    use tempfile::TempDir;

    fn read_input(path: impl Into<String>) -> ThoughtsReadInput {
        ThoughtsReadInput {
            file_path: path.into(),
            offset: None,
            limit: None,
        }
    }

    #[test]
    fn specialized_read_supports_files_and_directories() {
        let base = TempDir::new().unwrap();
        std::fs::create_dir_all(base.path().join("plans")).unwrap();
        std::fs::write(base.path().join("plans/approved.md"), "approved\n").unwrap();

        let file = read_from_base(base.path(), &read_input("plans/approved.md")).unwrap();
        assert!(file.contains("1: approved"));
        let directory = read_from_base(base.path(), &read_input("plans")).unwrap();
        assert!(directory.contains("approved.md"));
    }

    #[test]
    fn specialized_read_reports_missing_paths_as_not_found() {
        let base = TempDir::new().unwrap();

        let error = read_from_base(base.path(), &read_input("missing.md")).unwrap_err();

        assert!(matches!(error, ToolError::NotFound(_)));
    }

    #[test]
    fn specialized_read_rejects_absolute_and_traversal_escape() {
        let base = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        for path in [
            outside.path().join("secret.txt").display().to_string(),
            String::from("../secret.txt"),
        ] {
            assert!(read_from_base(base.path(), &read_input(path)).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn specialized_read_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let base = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
        symlink(
            outside.path().join("secret.txt"),
            base.path().join("link.txt"),
        )
        .unwrap();
        assert!(read_from_base(base.path(), &read_input("link.txt")).is_err());
    }

    #[test]
    fn specialized_read_reuses_size_and_utf8_bounds() {
        let base = TempDir::new().unwrap();
        let large = std::fs::File::create(base.path().join("large.txt")).unwrap();
        large
            .set_len(workspace_tools::tools::MAX_READ_BYTES + 1)
            .unwrap();
        assert!(read_from_base(base.path(), &read_input("large.txt")).is_err());

        std::fs::write(base.path().join("invalid.txt"), [0xFF]).unwrap();
        assert!(read_from_base(base.path(), &read_input("invalid.txt")).is_err());
    }

    #[tokio::test]
    async fn get_template_fails_when_readiness_fails() {
        let tool = GetTemplateTool {
            readiness: ThoughtsMcpReadinessGate::new_with_check(|| {
                Box::pin(async { anyhow::bail!("sentinel readiness failure") })
            }),
        };

        let error = tool
            .call(
                GetTemplateInput {
                    template: TemplateType::Plan,
                },
                &ToolContext::default(),
            )
            .await
            .unwrap_err();

        match error {
            ToolError::Internal(message) => assert!(message.contains("sentinel readiness failure")),
            other => panic!("expected internal readiness error, got {other:?}"),
        }
    }
}
