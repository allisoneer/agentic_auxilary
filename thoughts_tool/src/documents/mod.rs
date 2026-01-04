//! Library-level document management for thoughts_tool.
//!
//! This module provides reusable functions for writing and listing documents,
//! and is used by both the MCP layer and other crates that depend on thoughts_tool.

use crate::error::{Result as TResult, ThoughtsError};
use crate::utils::validation::validate_simple_filename;
use crate::workspace::{ActiveWork, ensure_active_work};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
    /// Returns the path for this document type's directory within ActiveWork.
    pub fn subdir<'a>(&self, aw: &'a ActiveWork) -> &'a PathBuf {
        match self {
            DocumentType::Research => &aw.research,
            DocumentType::Plan => &aw.plans,
            DocumentType::Artifact => &aw.artifacts,
            DocumentType::Log => &aw.logs,
        }
    }

    /// Returns the plural directory name (for physical directory paths).
    /// Note: serde serialization uses singular forms ("plan", "artifact", "research", "log"),
    /// while physical directories use plural forms ("plans", "artifacts", "research", "logs").
    /// This matches conventional filesystem naming while keeping API values consistent.
    pub fn subdir_name(&self) -> &'static str {
        match self {
            DocumentType::Research => "research",
            DocumentType::Plan => "plans",
            DocumentType::Artifact => "artifacts",
            DocumentType::Log => "logs",
        }
    }

    /// Returns the singular label for this document type (used in output/reporting).
    pub fn singular_label(&self) -> &'static str {
        match self {
            DocumentType::Research => "research",
            DocumentType::Plan => "plan",
            DocumentType::Artifact => "artifact",
            DocumentType::Log => "log",
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
            "research" => Ok(DocumentType::Research),
            "plan" | "plans" => Ok(DocumentType::Plan),
            "artifact" | "artifacts" => Ok(DocumentType::Artifact),
            "log" | "logs" => Ok(DocumentType::Log), // accepts both for backward compat
            other => Err(serde::de::Error::custom(format!(
                "invalid doc_type '{}'; expected research|plan(s)|artifact(s)|log(s)",
                other
            ))),
        }
    }
}

/// Result of successfully writing a document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteDocumentOk {
    pub path: String,
    pub bytes_written: u64,
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

/// Write a document to the active work directory.
///
/// # Arguments
/// * `doc_type` - The type of document (research, plan, artifact, logs)
/// * `filename` - The filename (validated for safety)
/// * `content` - The content to write
///
/// # Returns
/// A `WriteDocumentOk` with the path and bytes written on success.
pub fn write_document(
    doc_type: DocumentType,
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

    Ok(WriteDocumentOk {
        path: format!(
            "./thoughts/{}/{}/{}",
            aw.dir_name,
            doc_type.subdir_name(),
            filename
        ),
        bytes_written,
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
pub fn list_documents(subdir: Option<DocumentType>) -> TResult<ActiveDocuments> {
    let aw = ensure_active_work()?;
    let base = format!("./thoughts/{}", aw.dir_name);

    // Determine which subdirs to scan
    // Tuple: (singular_label for doc_type output, plural_dirname for paths, PathBuf)
    let sets: Vec<(&str, &str, PathBuf)> = match subdir {
        Some(ref d) => {
            vec![(d.singular_label(), d.subdir_name(), d.subdir(&aw).clone())]
        }
        None => vec![
            ("research", "research", aw.research.clone()),
            ("plan", "plans", aw.plans.clone()),
            ("artifact", "artifacts", aw.artifacts.clone()),
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
                    .map(|t| t.into())
                    .unwrap_or_else(|_| Utc::now());
                let file_name = entry.file_name().to_string_lossy().to_string();
                files.push(DocumentInfo {
                    path: format!("{}/{}/{}", base, dirname, file_name),
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
/// directly (e.g., agentic_logging).
///
/// # Returns
/// The absolute path to the logs directory.
pub fn active_logs_dir() -> TResult<PathBuf> {
    let aw = ensure_active_work()?;
    if !aw.logs.exists() {
        std::fs::create_dir_all(&aw.logs)?;
    }
    Ok(aw.logs.clone())
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
}
