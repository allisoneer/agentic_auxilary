//! Library-level document management for `thoughts_tool`.
//!
//! This module provides reusable functions for writing and listing documents,
//! and is used by both the MCP layer and other crates that depend on `thoughts_tool`.

use crate::error::Result as TResult;
use crate::error::ThoughtsError;
use crate::repo_identity::RepoIdentity;
use crate::utils::validation::validate_simple_filename;
use crate::workspace::ActiveWork;
use crate::workspace::ensure_active_work;
use atomicwrites::AtomicFile;
use atomicwrites::OverwriteBehavior;
use chrono::DateTime;
use chrono::Utc;
use percent_encoding::AsciiSet;
use percent_encoding::CONTROLS;
use percent_encoding::utf8_percent_encode;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

/// Document type categories for thoughts workspace.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Research,
    Plan,
    Artifact,
    Log,
}

impl DocumentType {
    /// Returns the path for this document type's directory within `ActiveWork`.
    pub fn subdir<'a>(&self, aw: &'a ActiveWork) -> &'a PathBuf {
        match self {
            Self::Research => &aw.research,
            Self::Plan => &aw.plans,
            Self::Artifact => &aw.artifacts,
            Self::Log => &aw.logs,
        }
    }

    /// Returns the plural directory name (for physical directory paths).
    /// Note: serde serialization uses singular forms ("plan", "artifact", "research", "log"),
    /// while physical directories use plural forms ("plans", "artifacts", "research", "logs").
    /// This matches conventional filesystem naming while keeping API values consistent.
    pub fn subdir_name(&self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Plan => "plans",
            Self::Artifact => "artifacts",
            Self::Log => "logs",
        }
    }

    /// Returns the singular label for this document type (used in output/reporting).
    pub fn singular_label(&self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Plan => "plan",
            Self::Artifact => "artifact",
            Self::Log => "log",
        }
    }
}

// Custom deserializer: accept singular/plural in a case-insensitive manner
impl<'de> serde::Deserialize<'de> for DocumentType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let norm = s.trim().to_ascii_lowercase();
        match norm.as_str() {
            "research" => Ok(Self::Research),
            "plan" | "plans" => Ok(Self::Plan),
            "artifact" | "artifacts" => Ok(Self::Artifact),
            "log" | "logs" => Ok(Self::Log), // accepts both for backward compat
            other => Err(serde::de::Error::custom(format!(
                "invalid doc_type '{other}'; expected research|plan(s)|artifact(s)|log(s)"
            ))),
        }
    }
}

/// Result of successfully writing a document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteDocumentOk {
    pub path: String,
    pub bytes_written: u64,
    /// GitHub URL for the document (available after sync).
    /// None if the remote is not GitHub-hosted or URL couldn't be computed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_url: Option<String>,
}

/// Metadata about a single document file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocumentInfo {
    pub path: String,
    pub doc_type: String,
    pub size: u64,
    pub modified: String,
}

/// Result of listing documents in the active work directory.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActiveDocuments {
    pub base: String,
    pub files: Vec<DocumentInfo>,
}

/// Compute GitHub blob URL if the remote is GitHub-hosted.
///
/// Returns None if:
/// - No remote URL is available
/// - No git ref is available
/// - The remote is not GitHub-hosted
/// - The URL couldn't be parsed
const GITHUB_PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

fn encode_path_segment(value: &str) -> String {
    value
        .split('/')
        .map(|segment| utf8_percent_encode(segment, GITHUB_PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn compute_github_url(
    remote_url: Option<&str>,
    repo_subpath: Option<&str>,
    git_ref: Option<&str>,
    dir_name: &str,
    doc_type: &DocumentType,
    filename: &str,
) -> Option<String> {
    let remote = remote_url?;
    let git_ref = git_ref?;
    let identity = RepoIdentity::parse(remote).ok()?;

    // Only generate URLs for GitHub
    if identity.host != "github.com" {
        return None;
    }

    // Guard against single-segment remotes that produce empty org_path
    if identity.org_path.is_empty() {
        return None;
    }

    // Build the path within the repo
    // Structure: {subpath}/{dir_name}/{doc_type_dir}/{filename}
    let mut path_parts = Vec::new();
    if let Some(subpath) = repo_subpath {
        let subpath = subpath.trim().trim_matches('/');
        if !subpath.is_empty() {
            path_parts.push(encode_path_segment(subpath));
        }
    }
    path_parts.push(encode_path_segment(dir_name));
    path_parts.push(doc_type.subdir_name().to_string());
    path_parts.push(encode_path_segment(filename));

    let path_in_repo = path_parts.join("/");

    Some(format!(
        "https://github.com/{}/{}/blob/{}/{}",
        encode_path_segment(&identity.org_path),
        encode_path_segment(&identity.repo),
        encode_path_segment(git_ref),
        path_in_repo
    ))
}

/// Write a document to the active work directory.
///
/// # Arguments
/// * `doc_type` - The type of document (research, plan, artifact, log)
/// * `filename` - The filename (validated for safety)
/// * `content` - The content to write
///
/// # Returns
/// A `WriteDocumentOk` with the path, bytes written, and optional GitHub URL on success.
pub fn write_document(
    doc_type: &DocumentType,
    filename: &str,
    content: &str,
) -> TResult<WriteDocumentOk> {
    validate_simple_filename(filename)?;
    let aw = ensure_active_work()?;
    let dir = doc_type.subdir(&aw);
    let target = dir.join(filename);
    let bytes_written = content.len() as u64;

    AtomicFile::new(&target, OverwriteBehavior::AllowOverwrite)
        .write(|f| std::io::Write::write_all(f, content.as_bytes()))
        .map_err(|e| ThoughtsError::Io(std::io::Error::other(e)))?;

    let github_url = compute_github_url(
        aw.remote_url.as_deref(),
        aw.repo_subpath.as_deref(),
        aw.thoughts_git_ref.as_deref(),
        &aw.dir_name,
        doc_type,
        filename,
    );

    Ok(WriteDocumentOk {
        path: format!(
            "./thoughts/{}/{}/{}",
            aw.dir_name,
            doc_type.subdir_name(),
            filename
        ),
        bytes_written,
        github_url,
    })
}

/// List documents in the active work directory.
///
/// # Arguments
/// * `subdir` - Optional filter for a specific document type. If None, lists research, plans, artifacts
///   (but NOT logs by default - logs must be explicitly requested).
///
/// # Returns
/// An `ActiveDocuments` with the base path and list of files.
pub fn list_documents(subdir: Option<&DocumentType>) -> TResult<ActiveDocuments> {
    let aw = ensure_active_work()?;
    let base = format!("./thoughts/{}", aw.dir_name);

    // Determine which subdirs to scan
    // Tuple: (singular_label for doc_type output, plural_dirname for paths, PathBuf)
    let sets: Vec<(&str, &str, PathBuf)> = match subdir {
        Some(d) => {
            vec![(d.singular_label(), d.subdir_name(), d.subdir(&aw).clone())]
        }
        None => vec![
            ("research", "research", aw.research.clone()),
            ("plan", "plans", aw.plans.clone()),
            ("artifact", "artifacts", aw.artifacts),
            // Do NOT include logs by default - must be explicitly requested
        ],
    };

    let mut files = Vec::new();
    for (singular_label, dirname, dir) in sets {
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            if meta.is_file() {
                let modified: DateTime<Utc> = meta
                    .modified()
                    .map_or_else(|_| Utc::now(), std::convert::Into::into);
                let file_name = entry.file_name().to_string_lossy().to_string();
                files.push(DocumentInfo {
                    path: format!("{base}/{dirname}/{file_name}"),
                    doc_type: singular_label.to_string(),
                    size: meta.len(),
                    modified: modified.to_rfc3339(),
                });
            }
        }
    }

    Ok(ActiveDocuments { base, files })
}

/// Get the path to the logs directory in the active work, ensuring it exists.
///
/// This is a convenience function for other crates that need to write log files
/// directly (e.g., `agentic_logging`).
///
/// # Returns
/// The absolute path to the logs directory.
pub fn active_logs_dir() -> TResult<PathBuf> {
    let aw = ensure_active_work()?;
    if !aw.logs.exists() {
        std::fs::create_dir_all(&aw.logs)?;
    }
    Ok(aw.logs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_type_deserialize_singular() {
        let research: DocumentType = serde_json::from_str("\"research\"").unwrap();
        assert!(matches!(research, DocumentType::Research));

        let plan: DocumentType = serde_json::from_str("\"plan\"").unwrap();
        assert!(matches!(plan, DocumentType::Plan));

        let artifact: DocumentType = serde_json::from_str("\"artifact\"").unwrap();
        assert!(matches!(artifact, DocumentType::Artifact));

        let log: DocumentType = serde_json::from_str("\"log\"").unwrap();
        assert!(matches!(log, DocumentType::Log));
    }

    #[test]
    fn test_document_type_deserialize_plural() {
        let plans: DocumentType = serde_json::from_str("\"plans\"").unwrap();
        assert!(matches!(plans, DocumentType::Plan));

        let artifacts: DocumentType = serde_json::from_str("\"artifacts\"").unwrap();
        assert!(matches!(artifacts, DocumentType::Artifact));

        let logs: DocumentType = serde_json::from_str("\"logs\"").unwrap();
        assert!(matches!(logs, DocumentType::Log));
    }

    #[test]
    fn test_document_type_deserialize_case_insensitive() {
        let plan: DocumentType = serde_json::from_str("\"PLAN\"").unwrap();
        assert!(matches!(plan, DocumentType::Plan));

        let research: DocumentType = serde_json::from_str("\"Research\"").unwrap();
        assert!(matches!(research, DocumentType::Research));

        let log: DocumentType = serde_json::from_str("\"LOG\"").unwrap();
        assert!(matches!(log, DocumentType::Log));

        let logs: DocumentType = serde_json::from_str("\"LOGS\"").unwrap();
        assert!(matches!(logs, DocumentType::Log));
    }

    #[test]
    fn test_document_type_deserialize_invalid() {
        let result: Result<DocumentType, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid doc_type"));
    }

    #[test]
    fn test_document_type_serialize() {
        let plan = DocumentType::Plan;
        let serialized = serde_json::to_string(&plan).unwrap();
        assert_eq!(serialized, "\"plan\"");

        let artifact = DocumentType::Artifact;
        let serialized = serde_json::to_string(&artifact).unwrap();
        assert_eq!(serialized, "\"artifact\"");

        let log = DocumentType::Log;
        let serialized = serde_json::to_string(&log).unwrap();
        assert_eq!(serialized, "\"log\"");
    }

    #[test]
    fn test_subdir_names() {
        assert_eq!(DocumentType::Research.subdir_name(), "research");
        assert_eq!(DocumentType::Plan.subdir_name(), "plans");
        assert_eq!(DocumentType::Artifact.subdir_name(), "artifacts");
        assert_eq!(DocumentType::Log.subdir_name(), "logs");
    }

    #[test]
    fn test_singular_labels() {
        assert_eq!(DocumentType::Research.singular_label(), "research");
        assert_eq!(DocumentType::Plan.singular_label(), "plan");
        assert_eq!(DocumentType::Artifact.singular_label(), "artifact");
        assert_eq!(DocumentType::Log.singular_label(), "log");
    }

    #[test]
    fn test_compute_github_url_ssh() {
        let url = compute_github_url(
            Some("git@github.com:org/repo.git"),
            None,
            Some("main"),
            "main",
            &DocumentType::Research,
            "doc.md",
        );
        assert_eq!(
            url,
            Some("https://github.com/org/repo/blob/main/main/research/doc.md".to_string())
        );
    }

    #[test]
    fn test_compute_github_url_https() {
        let url = compute_github_url(
            Some("https://github.com/org/repo.git"),
            Some("docs/thoughts"),
            Some("main"),
            "feature-branch",
            &DocumentType::Plan,
            "plan.md",
        );
        assert_eq!(
            url,
            Some(
                "https://github.com/org/repo/blob/main/docs/thoughts/feature-branch/plans/plan.md"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_compute_github_url_non_github() {
        let url = compute_github_url(
            Some("git@gitlab.com:org/repo.git"),
            None,
            Some("main"),
            "main",
            &DocumentType::Research,
            "doc.md",
        );
        assert_eq!(url, None);
    }

    #[test]
    fn test_compute_github_url_none_remote() {
        let url = compute_github_url(
            None,
            None,
            Some("main"),
            "main",
            &DocumentType::Research,
            "doc.md",
        );
        assert_eq!(url, None);
    }

    #[test]
    fn test_compute_github_url_no_subpath() {
        let url = compute_github_url(
            Some("git@github.com:General-Wisdom/thoughts.git"),
            None,
            Some("main"),
            "allison-feature",
            &DocumentType::Artifact,
            "test.md",
        );
        assert_eq!(
            url,
            Some("https://github.com/General-Wisdom/thoughts/blob/main/allison-feature/artifacts/test.md".to_string())
        );
    }

    #[test]
    fn test_compute_github_url_empty_org_path() {
        // Single-segment remotes produce empty org_path; should return None
        // to avoid malformed URLs like https://github.com//repo/...
        let url = compute_github_url(
            Some("git@github.com:repo.git"),
            None,
            Some("main"),
            "main",
            &DocumentType::Research,
            "doc.md",
        );
        assert_eq!(url, None);
    }

    #[test]
    fn test_compute_github_url_slash_branch() {
        let url = compute_github_url(
            Some("git@github.com:org/repo.git"),
            None,
            Some("main"),
            "feature/login",
            &DocumentType::Research,
            "notes.md",
        );
        assert_eq!(
            url,
            Some(
                "https://github.com/org/repo/blob/main/feature/login/research/notes.md".to_string()
            )
        );
    }

    #[test]
    fn test_compute_github_url_special_chars() {
        let url = compute_github_url(
            Some("git@github.com:org/repo.git"),
            None,
            Some("main"),
            "feat#1%",
            &DocumentType::Plan,
            "plan.md",
        );
        assert_eq!(
            url,
            Some("https://github.com/org/repo/blob/main/feat%231%25/plans/plan.md".to_string())
        );
    }

    #[test]
    fn test_compute_github_url_detached_head() {
        let url = compute_github_url(
            Some("git@github.com:org/repo.git"),
            None,
            None,
            "some-branch",
            &DocumentType::Research,
            "doc.md",
        );
        assert_eq!(url, None);
    }

    #[test]
    fn test_compute_github_url_space_in_branch() {
        let url = compute_github_url(
            Some("git@github.com:org/repo.git"),
            None,
            Some("main"),
            "my branch",
            &DocumentType::Artifact,
            "out.md",
        );
        assert_eq!(
            url,
            Some("https://github.com/org/repo/blob/main/my%20branch/artifacts/out.md".to_string())
        );
    }
}
