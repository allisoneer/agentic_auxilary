use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

// Data types for MCP tools (formatting implementations in src/fmt.rs)

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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemplateResponse {
    pub template_type: TemplateType,
}

// Note: Tool implementations are in thoughts-mcp-tools crate using agentic-tools framework.

/// Public adapter for add_reference implementation.
///
/// This function is callable by agentic-tools wrappers. It contains the actual
/// logic for adding a GitHub repository as a reference.
///
/// # Arguments
/// * `url` - HTTPS GitHub URL (https://github.com/org/repo or .git) or generic https://*.git clone URL
/// * `description` - Optional description for why this reference was added
///
/// # Returns
/// `AddReferenceOk` on success, `anyhow::Error` on failure.
pub async fn add_reference_impl_adapter(
    url: String,
    description: Option<String>,
) -> Result<AddReferenceOk> {
    let input_url = url.trim().to_string();

    // Validate URL per MCP HTTPS-only rules
    validate_reference_url_https_only(&input_url)
        .context("invalid input: URL failed HTTPS validation")?;

    // Parse org/repo; safe after validation
    let (org, repo) = extract_org_repo_from_url(&input_url)
        .context("invalid input: failed to extract org/repo from URL")?;

    // Resolve repo root and config manager
    let repo_root =
        get_control_repo_root(&std::env::current_dir().context("failed to get current directory")?)
            .context("failed to get control repo root")?;

    let mgr = RepoConfigManager::new(repo_root.clone());
    let mut cfg = mgr
        .ensure_v2_default()
        .context("failed to ensure v2 config")?;

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
    let this_key =
        canonical_reference_key(&input_url).context("invalid input: failed to canonicalize URL")?;
    let already_existed = existing_keys.contains(&this_key);

    // Compute paths for response
    let ds = mgr
        .load_desired_state()
        .context("failed to load desired state")?
        .ok_or_else(|| anyhow::anyhow!("not found: no repository configuration found"))?;
    let mount_path = format!("{}/{}/{}", ds.mount_dirs.references, org, repo);
    let mount_target = repo_root
        .join(".thoughts-data")
        .join(&mount_path)
        .to_string_lossy()
        .to_string();

    // Capture pre-sync mapping status
    let repo_mapping =
        RepoMappingManager::new().context("failed to create repo mapping manager")?;
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
            .context("failed to save config")?;
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
    let repo_mapping_post =
        RepoMappingManager::new().context("failed to create repo mapping manager")?;
    let post_mapping = repo_mapping_post
        .resolve_url(&input_url)
        .ok()
        .flatten()
        .map(|p| p.to_string_lossy().to_string());
    let cloned = pre_mapping.is_none() && post_mapping.is_some();

    // Determine mounted by listing active mounts
    let platform = detect_platform().context("failed to detect platform")?;
    let mount_manager = get_mount_manager(&platform).context("failed to get mount manager")?;
    let active = mount_manager
        .list_mounts()
        .await
        .context("failed to list mounts")?;
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

// Note: MCP server implementation moved to thoughts-mcp-tools crate using agentic-tools framework.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::{ActiveDocuments, DocumentInfo, WriteDocumentOk};
    use crate::utils::human_size;
    use agentic_tools_core::fmt::{TextFormat, TextOptions};

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
            path: "./thoughts/feat/research/a.md".into(),
            bytes_written: 2048,
        };
        let text = ok.fmt_text(&TextOptions::default());
        assert!(text.contains("2.0 KB"));
        assert!(text.contains("\u{2713} Created")); // ✓
        assert!(text.contains("./thoughts/feat/research/a.md"));
    }

    #[test]
    fn test_active_documents_empty() {
        let docs = ActiveDocuments {
            base: "./thoughts/x".into(),
            files: vec![],
        };
        let s = docs.fmt_text(&TextOptions::default());
        assert!(s.contains("<none>"));
        assert!(s.contains("./thoughts/x"));
    }

    #[test]
    fn test_active_documents_with_files() {
        let docs = ActiveDocuments {
            base: "./thoughts/feature".into(),
            files: vec![DocumentInfo {
                path: "./thoughts/feature/research/test.md".into(),
                doc_type: "research".into(),
                size: 1024,
                modified: "2025-10-15T12:00:00Z".into(),
            }],
        };
        let text = docs.fmt_text(&TextOptions::default());
        assert!(text.contains("research/test.md"));
        assert!(text.contains("2025-10-15 12:00 UTC"));
    }

    // Note: DocumentType serde tests are in crate::documents::tests

    #[test]
    fn test_references_list_empty() {
        let refs = ReferencesList {
            base: "references".into(),
            entries: vec![],
        };
        let s = refs.fmt_text(&TextOptions::default());
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
        let text = refs.fmt_text(&TextOptions::default());
        assert!(text.contains("org/repo1"));
        assert!(text.contains("org/repo2"));
        assert!(!text.contains("\u{2014}")); // No em-dash separator
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
        let text = refs.fmt_text(&TextOptions::default());
        assert!(text.contains("org/repo1 \u{2014} First repo")); // em-dash
        assert!(text.contains("org/repo2 \u{2014} Second repo"));
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
        let s = ok.fmt_text(&TextOptions::default());
        assert!(s.contains("\u{2713} Added reference")); // ✓
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
        let s = ok.fmt_text(&TextOptions::default());
        assert!(s.contains("\u{2713} Reference already exists (idempotent)"));
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
        let s = ok.fmt_text(&TextOptions::default());
        assert!(s.contains("Mapping: <none>"));
        assert!(s.contains("Mounted: false"));
        assert!(s.contains("- Clone failed"));
    }

    #[test]
    fn test_template_response_format_research() {
        let resp = TemplateResponse {
            template_type: TemplateType::Research,
        };
        let s = resp.fmt_text(&TextOptions::default());
        assert!(s.starts_with("Here is the research template:"));
        assert!(s.contains("```markdown"));
        // spot-check content from the research template
        assert!(s.contains("# Research: [Topic]"));
        // research guidance presence
        assert!(s.contains("Stop. Before writing this document"));
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
