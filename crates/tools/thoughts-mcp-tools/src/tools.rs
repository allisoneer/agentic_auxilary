//! Tool wrappers for thoughts_tool using agentic-tools-core.
//!
//! Each tool wraps the corresponding functionality from the thoughts_tool
//! library with logging identical to the MCP implementation.

use agentic_logging::CallTimer;
use agentic_tools_core::{Tool, ToolContext, ToolError};
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Deserialize;

use thoughts_tool::config::{RepoConfigManager, extract_org_repo_from_url};
use thoughts_tool::documents::{
    ActiveDocuments, DocumentType, WriteDocumentOk, list_documents, write_document,
};
use thoughts_tool::git::utils::get_control_repo_root;
use thoughts_tool::mcp::{
    AddReferenceOk, ReferenceItem, ReferencesList, TemplateResponse, TemplateType,
    add_reference_impl_adapter,
};
use thoughts_tool::utils::logging::log_tool_call;

/// Map anyhow::Error to agentic_tools_core::ToolError.
///
/// Uses string pattern matching to categorize errors appropriately.
fn map_anyhow_to_tool_error(e: anyhow::Error) -> ToolError {
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

// ============================================================================
// WriteDocument Tool
// ============================================================================

/// Input for the write_document tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WriteDocumentInput {
    /// Document type categories for thoughts workspace.
    pub doc_type: DocumentType,
    /// Filename for the document.
    pub filename: String,
    /// Content to write to the document.
    pub content: String,
}

/// Tool for writing documents to the active work directory.
#[derive(Clone)]
pub struct WriteDocumentTool;

impl Tool for WriteDocumentTool {
    type Input = WriteDocumentInput;
    type Output = WriteDocumentOk;
    const NAME: &'static str = "write_document";
    const DESCRIPTION: &'static str = "Write markdown to the active work directory";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "doc_type": input.doc_type.singular_label(),
                "filename": &input.filename,
            });

            let result = write_document(input.doc_type, &input.filename, &input.content);

            match &result {
                Ok(ok) => {
                    let summary = serde_json::json!({
                        "path": &ok.path,
                        "bytes_written": ok.bytes_written,
                    });
                    log_tool_call(
                        &timer,
                        "write_document",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "write_document",
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

/// Input for the list_active_documents tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListActiveDocumentsInput {
    /// Optional subdirectory filter by document type.
    #[serde(default)]
    pub subdir: Option<DocumentType>,
}

/// Tool for listing files in the active work directory.
#[derive(Clone)]
pub struct ListActiveDocumentsTool;

impl Tool for ListActiveDocumentsTool {
    type Input = ListActiveDocumentsInput;
    type Output = ActiveDocuments;
    const NAME: &'static str = "list_active_documents";
    const DESCRIPTION: &'static str = "List files in the current active work directory";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "subdir": input.subdir.as_ref().map(|d| format!("{:?}", d).to_lowercase()),
            });

            let result = list_documents(input.subdir);

            match &result {
                Ok(docs) => {
                    let summary = serde_json::json!({
                        "base": &docs.base,
                        "files_count": docs.files.len(),
                    });
                    log_tool_call(
                        &timer,
                        "list_active_documents",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "list_active_documents",
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

/// Input for the list_references tool.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
pub struct ListReferencesInput {}

/// Tool for listing reference repository directory paths.
#[derive(Clone)]
pub struct ListReferencesTool;

impl Tool for ListReferencesTool {
    type Input = ListReferencesInput;
    type Output = ReferencesList;
    const NAME: &'static str = "list_references";
    const DESCRIPTION: &'static str =
        "List reference repository directory paths (references/org/repo)";

    fn call(
        &self,
        _input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({});

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
                        Ok((org, repo)) => format!("{}/{}", org, repo),
                        Err(_) => rm.remote.clone(),
                    };
                    entries.push(ReferenceItem {
                        path: format!("{}/{}", base, path),
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
                        "list_references",
                        req_json,
                        true,
                        None,
                        Some(summary),
                    );
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "list_references",
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

/// Input for the add_reference tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AddReferenceInput {
    /// HTTPS GitHub URL (https://github.com/org/repo) or generic https://*.git clone URL
    pub url: String,
    /// Optional description for why this reference was added
    #[serde(default)]
    pub description: Option<String>,
}

/// Tool for adding a GitHub repository as a reference.
#[derive(Clone)]
pub struct AddReferenceTool;

impl Tool for AddReferenceTool {
    type Input = AddReferenceInput;
    type Output = AddReferenceOk;
    const NAME: &'static str = "add_reference";
    const DESCRIPTION: &'static str = "Add a GitHub repository as a reference and ensure it is cloned and mounted. Input must be an HTTPS GitHub URL (https://github.com/org/repo or .git) or generic https://*.git clone URL. SSH URLs (git@\u{2026}) are rejected. Idempotent and safe to retry; first-time clones may take time.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "url": &input.url,
                "description": &input.description,
            });

            // Delegate to the shared adapter function
            let result = add_reference_impl_adapter(input.url, input.description)
                .await
                .map_err(map_anyhow_to_tool_error);

            match &result {
                Ok(ok) => {
                    let summary = serde_json::json!({
                        "org": &ok.org,
                        "repo": &ok.repo,
                        "already_existed": ok.already_existed,
                        "config_updated": ok.config_updated,
                        "cloned": ok.cloned,
                        "mounted": ok.mounted,
                    });
                    log_tool_call(&timer, "add_reference", req_json, true, None, Some(summary));
                }
                Err(e) => {
                    log_tool_call(
                        &timer,
                        "add_reference",
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

/// Input for the get_template tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GetTemplateInput {
    /// Which template to fetch (research, plan, requirements, pr_description)
    pub template: TemplateType,
}

/// Tool for retrieving compile-time embedded templates.
#[derive(Clone)]
pub struct GetTemplateTool;

impl Tool for GetTemplateTool {
    type Input = GetTemplateInput;
    type Output = TemplateResponse;
    const NAME: &'static str = "get_template";
    const DESCRIPTION: &'static str = "Return a compile-time embedded template (research, plan, requirements, pr_description) with usage guidance";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        Box::pin(async move {
            let timer = CallTimer::start();
            let req_json = serde_json::json!({
                "template": input.template.label(),
            });

            let result = TemplateResponse {
                template_type: input.template,
            };

            let summary = serde_json::json!({
                "template_type": result.template_type.label(),
            });
            log_tool_call(&timer, "get_template", req_json, true, None, Some(summary));

            Ok(result)
        })
    }
}
