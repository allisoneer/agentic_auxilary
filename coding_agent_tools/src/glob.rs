//! Glob-based file matching with sorting.

use crate::types::{GlobOutput, SortOrder};
use crate::walker::{self, BUILTIN_IGNORES};
use globset::Glob;
use ignore::WalkBuilder;
use std::path::Path;
use universal_tool_core::prelude::ToolError;

/// Configuration for glob search.
#[derive(Debug)]
pub struct GlobConfig {
    /// Root directory to search
    pub root: String,
    /// Glob pattern to match against
    pub pattern: String,
    /// Additional glob patterns to ignore (exclude)
    pub ignore_globs: Vec<String>,
    /// Include hidden files
    pub include_hidden: bool,
    /// Sort order: name or mtime
    pub sort: SortOrder,
    /// Max results to return (capped at 1000)
    pub head_limit: usize,
    /// Skip the first N results
    pub offset: usize,
}

/// Maximum allowed head_limit to prevent context bloat.
const MAX_HEAD_LIMIT: usize = 1000;

/// Entry with metadata for sorting.
#[derive(Debug)]
struct GlobEntry {
    rel_path: String,
    mtime: Option<std::time::SystemTime>,
}

/// Run glob search with the given configuration.
pub fn run(cfg: GlobConfig) -> Result<GlobOutput, ToolError> {
    // Validate root path
    let root_path = Path::new(&cfg.root);
    if !root_path.exists() {
        return Err(ToolError::invalid_input(format!(
            "Path does not exist: {}",
            cfg.root
        )));
    }

    // Validate and compile glob pattern
    let pattern_glob = Glob::new(&cfg.pattern).map_err(|e| {
        ToolError::invalid_input(format!("Invalid glob pattern '{}': {}", cfg.pattern, e))
    })?;
    let pattern_matcher = pattern_glob.compile_matcher();

    // Build ignore globset
    let ignore_gs = walker::build_ignore_globset(&cfg.ignore_globs)?;

    // Cap head_limit
    let head_limit = cfg.head_limit.min(MAX_HEAD_LIMIT);

    let mut warnings: Vec<String> = Vec::new();
    let mut entries: Vec<GlobEntry> = Vec::new();

    // Configure walker
    let mut builder = WalkBuilder::new(root_path);
    builder.hidden(!cfg.include_hidden);
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);
    builder.parents(false);
    builder.follow_links(false);

    // Apply custom ignore filter
    let root_clone = root_path.to_path_buf();
    let gs_clone = ignore_gs.clone();
    builder.filter_entry(move |entry| {
        let rel = entry
            .path()
            .strip_prefix(&root_clone)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if rel.is_empty() {
            return true;
        }
        !gs_clone.is_match(&rel)
    });

    for result in builder.build() {
        match result {
            Ok(entry) => {
                let path = entry.path();

                // Skip root directory itself
                if path == root_path {
                    continue;
                }

                let rel_path = path
                    .strip_prefix(root_path)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| path.to_string_lossy().to_string());

                // Double-check against ignore patterns
                if ignore_gs.is_match(&rel_path) {
                    continue;
                }

                // Check against builtin ignores
                let matches_builtin = BUILTIN_IGNORES.iter().any(|pattern| {
                    if let Ok(g) = Glob::new(pattern) {
                        g.compile_matcher().is_match(&rel_path)
                    } else {
                        false
                    }
                });
                if matches_builtin {
                    continue;
                }

                // Check if path matches the glob pattern
                if !pattern_matcher.is_match(&rel_path) {
                    continue;
                }

                // Get mtime for sorting if needed
                let mtime = if matches!(cfg.sort, SortOrder::Mtime) {
                    std::fs::metadata(path).and_then(|m| m.modified()).ok()
                } else {
                    None
                };

                entries.push(GlobEntry { rel_path, mtime });
            }
            Err(e) => {
                warnings.push(format!("Walk error: {}", e));
            }
        }
    }

    // Sort entries
    match cfg.sort {
        SortOrder::Name => {
            // Case-insensitive alphabetical
            entries.sort_by(|a, b| a.rel_path.to_lowercase().cmp(&b.rel_path.to_lowercase()));
        }
        SortOrder::Mtime => {
            // Newest first (reverse chronological)
            entries.sort_by(|a, b| {
                match (&b.mtime, &a.mtime) {
                    (Some(b_time), Some(a_time)) => b_time.cmp(a_time),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => {
                        // Fall back to name sort if no mtime
                        a.rel_path.to_lowercase().cmp(&b.rel_path.to_lowercase())
                    }
                }
            });
        }
    }

    // Apply pagination
    let total_count = entries.len();
    let paginated: Vec<String> = entries
        .into_iter()
        .skip(cfg.offset)
        .take(head_limit)
        .map(|e| e.rel_path)
        .collect();
    let has_more = total_count > cfg.offset + paginated.len();

    Ok(GlobOutput {
        root: cfg.root,
        entries: paginated,
        has_more,
        warnings,
    })
}
