//! Directory traversal with gitignore and custom pattern filtering.

use crate::types::{EntryKind, LsEntry, Show};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use std::path::Path;
use universal_tool_core::prelude::ToolError;

/// Built-in ignore patterns for common non-essential directories.
/// Each directory has two patterns: one for the directory itself and one for its contents.
pub const BUILTIN_IGNORES: &[&str] = &[
    // node_modules
    "**/node_modules",
    "**/node_modules/**",
    // __pycache__
    "**/__pycache__",
    "**/__pycache__/**",
    // dist/build
    "**/dist",
    "**/dist/**",
    "**/build",
    "**/build/**",
    // target (Rust)
    "**/target",
    "**/target/**",
    // vendor
    "**/vendor",
    "**/vendor/**",
    // bin/obj
    "**/bin",
    "**/bin/**",
    "**/obj",
    "**/obj/**",
    // IDE directories
    "**/.idea",
    "**/.idea/**",
    "**/.vscode",
    "**/.vscode/**",
    // Zig
    "**/.zig-cache",
    "**/.zig-cache/**",
    "**/zig-out",
    "**/zig-out/**",
    // Coverage
    "**/.coverage",
    "**/.coverage/**",
    "**/coverage",
    "**/coverage/**",
    // Temp
    "**/tmp",
    "**/tmp/**",
    "**/temp",
    "**/temp/**",
    // Cache
    "**/.cache",
    "**/.cache/**",
    "**/cache",
    "**/cache/**",
    // Logs
    "**/logs",
    "**/logs/**",
    // Python venvs
    "**/.venv",
    "**/.venv/**",
    "**/venv",
    "**/venv/**",
    "**/env",
    "**/env/**",
];

/// Build a GlobSet from built-in and user patterns.
fn build_globset(user_patterns: &[String]) -> Result<GlobSet, ToolError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in BUILTIN_IGNORES
        .iter()
        .copied()
        .chain(user_patterns.iter().map(String::as_str))
    {
        let glob = Glob::new(pattern).map_err(|e| {
            ToolError::invalid_input(format!("Invalid glob pattern '{}': {}", pattern, e))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| ToolError::internal(format!("Failed to build globset: {}", e)))
}

/// Configuration for directory walking.
pub struct WalkConfig<'a> {
    pub root: &'a Path,
    pub depth: u8,
    pub show: Show,
    pub user_ignores: &'a [String],
    pub include_hidden: bool,
}

/// Result of walking a directory.
pub struct WalkResult {
    pub entries: Vec<LsEntry>,
    pub warnings: Vec<String>,
}

/// List directory contents according to configuration.
pub fn list(cfg: &WalkConfig<'_>) -> Result<WalkResult, ToolError> {
    // Depth 0 = header only, no entries
    if cfg.depth == 0 {
        return Ok(WalkResult {
            entries: vec![],
            warnings: vec![],
        });
    }

    let globset = build_globset(cfg.user_ignores)?;

    // Configure the walker
    let mut builder = WalkBuilder::new(cfg.root);
    builder.max_depth(Some(cfg.depth as usize));
    builder.hidden(!cfg.include_hidden); // hidden(true) = SHOW hidden
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);
    builder.parents(false); // Critical: allows listing inside gitignored dirs
    builder.follow_links(false);

    // Apply custom ignore filter
    let root = cfg.root.to_path_buf();
    let gs = globset.clone();
    builder.filter_entry(move |entry| {
        let rel = entry
            .path()
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if rel.is_empty() {
            return true; // Always include root
        }
        !gs.is_match(&rel)
    });

    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for result in builder.build() {
        match result {
            Ok(entry) => {
                // Skip the root directory itself
                if entry.depth() == 0 {
                    continue;
                }

                let rel = entry
                    .path()
                    .strip_prefix(cfg.root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();

                // Double-check against globset (filter_entry might miss some)
                if globset.is_match(&rel) {
                    continue;
                }

                // Determine entry kind
                let file_type = entry.file_type();
                let kind = match file_type {
                    Some(ft) if ft.is_dir() => EntryKind::Dir,
                    Some(ft) if ft.is_file() => EntryKind::File,
                    Some(ft) if ft.is_symlink() => EntryKind::Symlink,
                    _ => {
                        // Fall back to symlink_metadata for edge cases
                        match std::fs::symlink_metadata(entry.path()) {
                            Ok(md) if md.file_type().is_symlink() => EntryKind::Symlink,
                            Ok(md) if md.is_dir() => EntryKind::Dir,
                            Ok(_) => EntryKind::File,
                            Err(err) => {
                                warnings.push(format!("Skipping {}: {}", rel, err));
                                continue;
                            }
                        }
                    }
                };

                // Apply show filter
                match cfg.show {
                    Show::Dirs if !matches!(kind, EntryKind::Dir) => continue,
                    Show::Files if matches!(kind, EntryKind::Dir) => continue,
                    _ => {}
                }

                entries.push(LsEntry { path: rel, kind });
            }
            Err(err) => {
                warnings.push(format!("Walk error: {}", err));
            }
        }
    }

    // Sort entries
    sort_entries(&mut entries, cfg.show);

    // Check for broken symlinks and add warnings
    for entry in &entries {
        if matches!(entry.kind, EntryKind::Symlink) {
            let full_path = cfg.root.join(&entry.path);
            if std::fs::metadata(&full_path).is_err() {
                warnings.push(format!("Broken symlink: {}", entry.path));
            }
        }
    }

    Ok(WalkResult { entries, warnings })
}

/// Sort entries according to show mode.
/// - For show=all: directories first, then files (both alphabetically)
/// - For show=files|dirs: plain alphabetical
fn sort_entries(entries: &mut [LsEntry], show: Show) {
    match show {
        Show::All => {
            entries.sort_by(|a, b| {
                match (&a.kind, &b.kind) {
                    // Directories come before files/symlinks
                    (EntryKind::Dir, EntryKind::File | EntryKind::Symlink) => {
                        std::cmp::Ordering::Less
                    }
                    (EntryKind::File | EntryKind::Symlink, EntryKind::Dir) => {
                        std::cmp::Ordering::Greater
                    }
                    // Same type: alphabetical (case-insensitive)
                    _ => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
                }
            });
        }
        _ => {
            // Plain alphabetical for files-only or dirs-only
            entries.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_ignores_compile() {
        let gs = build_globset(&[]).unwrap();
        assert!(gs.is_match("node_modules/foo.js"));
        assert!(gs.is_match("src/target/debug"));
        assert!(!gs.is_match("src/main.rs"));
    }

    #[test]
    fn user_patterns_work() {
        let gs = build_globset(&["*.log".into(), "dist/".into()]).unwrap();
        assert!(gs.is_match("app.log"));
        assert!(gs.is_match("dist/bundle.js"));
    }

    #[test]
    fn invalid_pattern_errors() {
        let result = build_globset(&["[invalid".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn sort_all_dirs_first() {
        let mut entries = vec![
            LsEntry {
                path: "zebra.txt".into(),
                kind: EntryKind::File,
            },
            LsEntry {
                path: "alpha".into(),
                kind: EntryKind::Dir,
            },
            LsEntry {
                path: "beta.rs".into(),
                kind: EntryKind::File,
            },
            LsEntry {
                path: "gamma".into(),
                kind: EntryKind::Dir,
            },
        ];
        sort_entries(&mut entries, Show::All);

        // Dirs first (alpha, gamma), then files (beta, zebra)
        assert_eq!(entries[0].path, "alpha");
        assert_eq!(entries[1].path, "gamma");
        assert_eq!(entries[2].path, "beta.rs");
        assert_eq!(entries[3].path, "zebra.txt");
    }

    #[test]
    fn sort_files_only_alphabetical() {
        let mut entries = vec![
            LsEntry {
                path: "zebra.txt".into(),
                kind: EntryKind::File,
            },
            LsEntry {
                path: "alpha.txt".into(),
                kind: EntryKind::File,
            },
        ];
        sort_entries(&mut entries, Show::Files);

        assert_eq!(entries[0].path, "alpha.txt");
        assert_eq!(entries[1].path, "zebra.txt");
    }
}
