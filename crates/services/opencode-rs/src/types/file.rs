//! File types for opencode_rs.

use serde::{Deserialize, Serialize};

/// A file in the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    /// File path relative to project root.
    pub path: String,
    /// File size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Whether this is a directory.
    #[serde(default)]
    pub is_directory: bool,
    /// Last modified timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<i64>,
}

/// File content response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// File path.
    pub path: String,
    /// File content.
    pub content: String,
    /// MIME type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    /// Encoding (e.g., "utf-8", "base64").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

/// File status in VCS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    /// File path.
    pub path: String,
    /// VCS status (added, modified, deleted, etc.).
    pub status: String,
    /// Whether the file is staged.
    #[serde(default)]
    pub staged: bool,
}

/// Request to list files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesRequest {
    /// Directory to list (relative to project root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether to recurse into subdirectories.
    #[serde(default)]
    pub recursive: bool,
    /// Maximum number of files to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Request to read file content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileRequest {
    /// File path to read.
    pub path: String,
    /// Start line (1-indexed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    /// End line (1-indexed, inclusive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
}
