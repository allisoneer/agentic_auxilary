use anyhow::Context;
use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Semaphore;

mod templates;

use crate::config::ReferenceEntry;
use crate::config::ReferenceMount;
use crate::config::RepoConfigManager;
use crate::config::RepoMappingManager;
use crate::config::extract_org_repo_from_url;
use crate::config::validation::canonical_reference_instance_key;
use crate::config::validation::validate_pinned_ref_full_name_new_input;
use crate::config::validation::validate_reference_url_https_only;
use crate::git::ref_key::encode_ref_key;
use crate::git::remote_refs::RemoteRef;
use crate::git::remote_refs::discover_remote_refs;
use crate::git::utils::get_control_repo_root;
use crate::mount::MountSpace;
use crate::mount::auto_mount::update_active_mounts;
use crate::mount::get_mount_manager;
use crate::platform::detect_platform;

const DEFAULT_REPO_REFS_LIMIT: usize = 100;
const MAX_REPO_REFS_LIMIT: usize = 200;
const REPO_REFS_MAX_CONCURRENCY: usize = 4;
const REPO_REFS_TIMEOUT_SECS: u64 = 20;

static REPO_REFS_SEM: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn find_matching_existing_reference(
    cfg: &crate::config::RepoConfigV2,
    input_url: &str,
    requested_ref_name: Option<&str>,
) -> Option<(String, Option<String>)> {
    let wanted = canonical_reference_instance_key(input_url, requested_ref_name).ok()?;

    for entry in &cfg.references {
        let (existing_url, existing_ref_name) = match entry {
            ReferenceEntry::Simple(url) => (url.as_str(), None),
            ReferenceEntry::WithMetadata(reference_mount) => (
                reference_mount.remote.as_str(),
                reference_mount.ref_name.as_deref(),
            ),
        };

        let Ok(existing_key) = canonical_reference_instance_key(existing_url, existing_ref_name)
        else {
            continue;
        };

        if existing_key == wanted {
            return Some((
                existing_url.to_string(),
                existing_ref_name.map(ToString::to_string),
            ));
        }
    }

    None
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
            Self::Research => "research",
            Self::Plan => "plan",
            Self::Requirements => "requirements",
            Self::PrDescription => "pr_description",
        }
    }
    pub fn content(&self) -> &'static str {
        match self {
            Self::Research => templates::RESEARCH_TEMPLATE_MD,
            Self::Plan => templates::PLAN_TEMPLATE_MD,
            Self::Requirements => templates::REQUIREMENTS_TEMPLATE_MD,
            Self::PrDescription => templates::PR_DESCRIPTION_TEMPLATE_MD,
        }
    }
    pub fn guidance(&self) -> &'static str {
        match self {
            Self::Research => templates::RESEARCH_GUIDANCE,
            Self::Plan => templates::PLAN_GUIDANCE,
            Self::Requirements => templates::REQUIREMENTS_GUIDANCE,
            Self::PrDescription => templates::PR_DESCRIPTION_GUIDANCE,
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
pub struct RepoRefsList {
    pub url: String,
    pub total: usize,
    pub truncated: bool,
    pub entries: Vec<RemoteRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddReferenceOk {
    pub url: String,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
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

fn repo_refs_semaphore() -> Arc<Semaphore> {
    Arc::clone(REPO_REFS_SEM.get_or_init(|| Arc::new(Semaphore::new(REPO_REFS_MAX_CONCURRENCY))))
}

fn get_repo_refs_blocking(input_url: String, limit: usize) -> Result<RepoRefsList> {
    let repo_root =
        get_control_repo_root(&std::env::current_dir().context("failed to get current directory")?)
            .context("failed to get control repo root")?;
    let mut refs = discover_remote_refs(&repo_root, &input_url)?;
    refs.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.oid.cmp(&b.oid))
            .then_with(|| a.peeled.cmp(&b.peeled))
    });

    let total = refs.len();
    let truncated = total > limit;
    refs.truncate(limit);

    Ok(RepoRefsList {
        url: input_url,
        total,
        truncated,
        entries: refs,
    })
}

async fn run_blocking_repo_refs_with_deadline<R, F>(
    sem: Arc<Semaphore>,
    timeout: Duration,
    op_label: String,
    work: F,
) -> Result<R>
where
    R: Send + 'static,
    F: FnOnce() -> Result<R> + Send + 'static,
{
    let deadline = tokio::time::Instant::now() + timeout;

    let permit = match tokio::time::timeout_at(deadline, sem.acquire_owned()).await {
        Ok(permit) => permit.context("semaphore unexpectedly closed")?,
        Err(_) => anyhow::bail!("timeout while waiting to start {op_label} after {timeout:?}"),
    };

    let mut handle = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    });

    if let Ok(joined) = tokio::time::timeout_at(deadline, &mut handle).await {
        joined.context("remote ref discovery task failed")?
    } else {
        tokio::spawn(async move {
            let _ = handle.await;
        });
        anyhow::bail!("timeout while {op_label} after {timeout:?}");
    }
}

pub async fn get_repo_refs_impl_adapter(url: String, limit: Option<usize>) -> Result<RepoRefsList> {
    let input_url = url.trim().to_string();
    validate_reference_url_https_only(&input_url)
        .context("invalid input: URL failed HTTPS validation")?;
    let limit = normalize_repo_ref_limit(limit)?;

    let sem = repo_refs_semaphore();
    let timeout = Duration::from_secs(REPO_REFS_TIMEOUT_SECS);
    let op_label = format!("discovering remote refs for {input_url}");
    let url_for_task = input_url.clone();

    run_blocking_repo_refs_with_deadline(sem, timeout, op_label, move || {
        get_repo_refs_blocking(url_for_task, limit)
    })
    .await
}

fn normalize_repo_ref_limit(limit: Option<usize>) -> Result<usize> {
    match limit.unwrap_or(DEFAULT_REPO_REFS_LIMIT) {
        0 => anyhow::bail!("invalid input: limit must be at least 1"),
        limit if limit > MAX_REPO_REFS_LIMIT => {
            anyhow::bail!("invalid input: limit must be at most {MAX_REPO_REFS_LIMIT}")
        }
        limit => Ok(limit),
    }
}

fn response_identity_url<'a>(
    input_url: &'a str,
    matched_existing: Option<&'a (String, Option<String>)>,
) -> &'a str {
    matched_existing.map_or(input_url, |(stored_url, _)| stored_url.as_str())
}

/// Public adapter for `add_reference` implementation.
///
/// This function is callable by agentic-tools wrappers. It contains the actual
/// logic for adding a GitHub repository as a reference.
///
/// # Arguments
/// * `url` - HTTPS GitHub URL (<https://github.com/org/repo> or .git) or generic https://*.git clone URL
/// * `description` - Optional description for why this reference was added
/// * `ref_name` - Optional full git ref name (for example refs/heads/main)
///
/// # Returns
/// `AddReferenceOk` on success, `anyhow::Error` on failure.
pub async fn add_reference_impl_adapter(
    url: String,
    description: Option<String>,
    ref_name: Option<String>,
) -> Result<AddReferenceOk> {
    let input_url = url.trim().to_string();
    let requested_ref_name = match ref_name {
        Some(ref_name) => {
            let trimmed = ref_name.trim();
            if trimmed.is_empty() {
                anyhow::bail!("invalid input: ref cannot be empty");
            }
            Some(trimmed.to_string())
        }
        None => None,
    };
    if let Some(ref_name) = requested_ref_name.as_deref()
        && let Err(e) = validate_pinned_ref_full_name_new_input(ref_name)
    {
        anyhow::bail!(
            "invalid input: ref must be a full ref name like 'refs/heads/main' or 'refs/tags/v1.2.3' \
(shorthand like 'main' is not supported). Details: {e}. \
Tip: call thoughts_get_repo_refs to discover full refs."
        );
    }

    // Validate URL per MCP HTTPS-only rules
    validate_reference_url_https_only(&input_url)
        .context("invalid input: URL failed HTTPS validation")?;

    // Resolve repo root and config manager
    let repo_root =
        get_control_repo_root(&std::env::current_dir().context("failed to get current directory")?)
            .context("failed to get control repo root")?;

    let mgr = RepoConfigManager::new(repo_root.clone());
    let mut cfg = mgr
        .ensure_v2_default()
        .context("failed to ensure v2 config")?;

    canonical_reference_instance_key(&input_url, requested_ref_name.as_deref())
        .context("invalid input: failed to canonicalize URL")?;
    let matched_existing =
        find_matching_existing_reference(&cfg, &input_url, requested_ref_name.as_deref());
    let already_existed = matched_existing.is_some();
    let effective_ref_name = matched_existing
        .as_ref()
        .and_then(|(_, ref_name)| ref_name.clone())
        .or_else(|| requested_ref_name.clone());
    let identity_url = response_identity_url(&input_url, matched_existing.as_ref());
    let (org, repo) = extract_org_repo_from_url(identity_url)
        .context("invalid input: failed to extract org/repo from URL")?;
    let ref_key = effective_ref_name
        .as_deref()
        .map(encode_ref_key)
        .transpose()?;

    // Compute paths for response
    let ds = mgr
        .load_desired_state()
        .context("failed to load desired state")?
        .ok_or_else(|| anyhow::anyhow!("not found: no repository configuration found"))?;
    let mount_space = MountSpace::Reference {
        org_path: org.clone(),
        repo: repo.clone(),
        ref_key: ref_key.clone(),
    };
    let mount_path = mount_space.relative_path(&ds.mount_dirs);
    let mount_target = repo_root
        .join(".thoughts-data")
        .join(&mount_path)
        .to_string_lossy()
        .to_string();

    // Capture pre-sync mapping status
    let repo_mapping =
        RepoMappingManager::new().context("failed to create repo mapping manager")?;
    let pre_mapping = repo_mapping
        .resolve_reference_url(&input_url, effective_ref_name.as_deref())
        .ok()
        .flatten()
        .map(|p| p.to_string_lossy().to_string());

    // Update config if new
    let mut config_updated = false;
    let mut warnings: Vec<String> = Vec::new();
    let description = description.and_then(|desc| {
        let trimmed = desc.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    if !already_existed {
        if description.is_some() || requested_ref_name.is_some() {
            cfg.references
                .push(ReferenceEntry::WithMetadata(ReferenceMount {
                    remote: input_url.clone(),
                    description: description.clone(),
                    ref_name: requested_ref_name.clone(),
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
    } else if description.is_some() || requested_ref_name.is_some() {
        warnings.push(
            "Reference already exists; metadata was not updated (use CLI to modify metadata)"
                .to_string(),
        );
    }

    // Always attempt to sync clone+mount (best-effort, no rollback)
    if let Err(e) = update_active_mounts().await {
        warnings.push(format!("Mount synchronization encountered an error: {e}"));
    }

    // Post-sync mapping status to infer cloning
    let repo_mapping_post =
        RepoMappingManager::new().context("failed to create repo mapping manager")?;
    let post_mapping = repo_mapping_post
        .resolve_reference_url(&input_url, effective_ref_name.as_deref())
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
    let target_canon = std::fs::canonicalize(&target_path).unwrap_or(target_path);
    let mut mounted = false;
    for mi in active {
        let canon = std::fs::canonicalize(&mi.target).unwrap_or_else(|_| mi.target.clone());
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
        ref_name: effective_ref_name,
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
    use crate::config::MountDirsV2;
    use crate::config::RepoConfigV2;
    use crate::documents::ActiveDocuments;
    use crate::documents::DocumentInfo;
    use crate::documents::WriteDocumentOk;
    use crate::utils::human_size;
    use agentic_tools_core::fmt::TextFormat;
    use agentic_tools_core::fmt::TextOptions;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;

    fn sample_remote_ref(name: &str) -> RemoteRef {
        RemoteRef {
            name: name.to_string(),
            oid: Some("abc123".to_string()),
            peeled: None,
            target: None,
        }
    }

    #[test]
    fn normalize_repo_ref_limit_defaults_and_validates() {
        assert_eq!(normalize_repo_ref_limit(None).unwrap(), 100);
        assert_eq!(normalize_repo_ref_limit(Some(1)).unwrap(), 1);
        assert!(normalize_repo_ref_limit(Some(0)).is_err());
        assert!(normalize_repo_ref_limit(Some(201)).is_err());
    }

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
            github_url: None,
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
    fn test_repo_ref_sorting_is_deterministic() {
        let mut refs = [
            sample_remote_ref("refs/tags/v2"),
            sample_remote_ref("refs/heads/main"),
        ];
        refs.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.target.cmp(&b.target))
                .then_with(|| a.oid.cmp(&b.oid))
                .then_with(|| a.peeled.cmp(&b.peeled))
        });
        assert_eq!(refs[0].name, "refs/heads/main");
        assert_eq!(refs[1].name, "refs/tags/v2");
    }

    #[tokio::test]
    async fn get_repo_refs_rejects_invalid_limit_async() {
        let err = get_repo_refs_impl_adapter("https://github.com/org/repo".into(), Some(0))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("limit must be at least 1"));
    }

    #[tokio::test]
    async fn get_repo_refs_rejects_ssh_url_async() {
        let err = get_repo_refs_impl_adapter("git@github.com:org/repo.git".into(), None)
            .await
            .unwrap_err();
        assert!(
            format!("{err:#}").to_lowercase().contains("ssh"),
            "unexpected error chain: {err:#}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repo_refs_deadline_includes_semaphore_acquire_time() {
        let sem = Arc::new(Semaphore::new(1));
        let _held = Arc::clone(&sem).acquire_owned().await.unwrap();
        let work_started = Arc::new(AtomicBool::new(false));
        let started = Arc::clone(&work_started);

        let err = run_blocking_repo_refs_with_deadline(
            sem,
            Duration::from_millis(10),
            "test operation".to_string(),
            move || {
                started.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("waiting to start test operation"));
        assert!(!work_started.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repo_refs_timeout_retains_permit_until_blocking_work_finishes() {
        let sem = Arc::new(Semaphore::new(1));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();

        let timed_out = tokio::spawn(run_blocking_repo_refs_with_deadline(
            Arc::clone(&sem),
            Duration::from_millis(20),
            "test operation".to_string(),
            move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                finished_tx.send(()).unwrap();
                Ok(())
            },
        ));

        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("blocking work should have started");

        let err = timed_out.await.unwrap().unwrap_err();
        assert!(err.to_string().contains("timeout while test operation"));
        assert_eq!(sem.available_permits(), 0, "permit should still be held");

        let blocked_err = run_blocking_repo_refs_with_deadline(
            Arc::clone(&sem),
            Duration::from_millis(10),
            "follow-up operation".to_string(),
            || Ok(()),
        )
        .await
        .unwrap_err();
        assert!(
            blocked_err
                .to_string()
                .contains("waiting to start follow-up operation"),
            "unexpected error: {blocked_err:#}"
        );

        release_tx.send(()).unwrap();
        finished_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("blocking work should finish after release");

        for _ in 0..20 {
            if sem.available_permits() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(
            sem.available_permits(),
            1,
            "permit should be released after blocking work completes"
        );

        run_blocking_repo_refs_with_deadline(
            sem,
            Duration::from_secs(1),
            "final operation".to_string(),
            || Ok(()),
        )
        .await
        .expect("follow-up work should succeed once permit is released");
    }

    #[test]
    fn test_repo_refs_list_format() {
        let refs = RepoRefsList {
            url: "https://github.com/org/repo".into(),
            total: 2,
            truncated: false,
            entries: vec![
                RemoteRef {
                    name: "refs/heads/main".into(),
                    oid: Some("abc123".into()),
                    peeled: None,
                    target: None,
                },
                RemoteRef {
                    name: "refs/tags/v1.0.0".into(),
                    oid: Some("def456".into()),
                    peeled: Some("fedcba".into()),
                    target: None,
                },
            ],
        };

        let text = refs.fmt_text(&TextOptions::default());
        assert!(text.contains("Remote refs for https://github.com/org/repo"));
        assert!(text.contains("refs/heads/main"));
        assert!(text.contains("oid=abc123"));
        assert!(text.contains("peeled=fedcba"));
    }

    #[test]
    fn test_add_reference_ok_format() {
        let ok = AddReferenceOk {
            url: "https://github.com/org/repo".into(),
            ref_name: Some("refs/heads/main".into()),
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
        assert!(s.contains("Ref: refs/heads/main"));
        assert!(s.contains("Cloned: true"));
        assert!(s.contains("Mounted: true"));
        assert!(s.contains("Warnings:\n- note"));
    }

    #[test]
    fn test_add_reference_ok_format_already_existed() {
        let ok = AddReferenceOk {
            url: "https://github.com/org/repo".into(),
            ref_name: None,
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
            ref_name: None,
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

    #[tokio::test]
    async fn add_reference_rejects_shorthand_ref_early() {
        let err = add_reference_impl_adapter(
            "https://github.com/org/repo".into(),
            None,
            Some("main".into()),
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("invalid input: ref must be a full ref name")
        );
    }

    #[tokio::test]
    async fn add_reference_rejects_refs_remotes_early() {
        let err = add_reference_impl_adapter(
            "https://github.com/org/repo".into(),
            None,
            Some("refs/remotes/origin/main".into()),
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("invalid input: ref must be a full ref name"),
            "unexpected error: {err:#}"
        );
        assert!(
            err.to_string().contains("refs/heads/main"),
            "unexpected error: {err:#}"
        );
    }

    #[tokio::test]
    async fn add_reference_rejects_bare_heads_prefix_early() {
        let err = add_reference_impl_adapter(
            "https://github.com/org/repo".into(),
            None,
            Some("refs/heads/".into()),
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("invalid input: ref must be a full ref name")
        );
    }

    #[tokio::test]
    async fn add_reference_rejects_bare_tags_prefix_early() {
        let err = add_reference_impl_adapter(
            "https://github.com/org/repo".into(),
            None,
            Some("refs/tags/".into()),
        )
        .await
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("invalid input: ref must be a full ref name")
        );
    }

    #[test]
    fn find_matching_existing_reference_returns_legacy_ref_name_when_equivalent() {
        let cfg = RepoConfigV2 {
            version: "2.0".into(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![ReferenceEntry::WithMetadata(ReferenceMount {
                remote: "https://github.com/org/repo".into(),
                description: None,
                ref_name: Some("refs/remotes/origin/main".into()),
            })],
        };

        let found = find_matching_existing_reference(
            &cfg,
            "https://github.com/org/repo",
            Some("refs/heads/main"),
        )
        .expect("should match by canonical identity");

        assert_eq!(found.0, "https://github.com/org/repo");
        assert_eq!(found.1.as_deref(), Some("refs/remotes/origin/main"));
    }

    #[test]
    fn idempotent_add_reference_response_uses_matched_stored_url_identity_for_paths() {
        let input_url = "https://github.com/org/repo";
        let stored_url = "https://github.com/Org/Repo";
        let matched_existing = Some((stored_url.to_string(), None));

        let identity_url = response_identity_url(input_url, matched_existing.as_ref());
        assert_eq!(identity_url, stored_url);

        let (org, repo) = extract_org_repo_from_url(identity_url).unwrap();
        let mount_dirs = MountDirsV2::default();
        let mount_space = MountSpace::Reference {
            org_path: org.clone(),
            repo: repo.clone(),
            ref_key: None,
        };

        assert_eq!(org, "Org");
        assert_eq!(repo, "Repo");
        assert_eq!(
            mount_space.relative_path(&mount_dirs),
            format!("{}/{}/{}", mount_dirs.references, org, repo)
        );
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
                "Embedded content unexpectedly empty for {t:?}"
            );
            assert!(
                !t.label().trim().is_empty(),
                "Label unexpectedly empty for {t:?}"
            );
        }
    }
}
