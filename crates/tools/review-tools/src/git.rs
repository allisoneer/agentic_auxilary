//! Git diff generation using git2.

use agentic_tools_core::ToolError;
use git2::{Diff, DiffFormat, DiffOptions, Repository};
use std::path::Path;

use crate::types::{DiffPage, DiffStats, ReviewDiffMode, ReviewSnapshot};

/// Base ref fallback order for finding merge-base.
const BASE_REF_FALLBACKS: &[&str] = &["origin/main", "origin/master", "main", "master"];

/// Build a review snapshot from the current repository state.
///
/// # Arguments
///
/// * `start` - Starting path for repository discovery
/// * `mode` - Diff mode (Default or Staged)
/// * `paths` - Optional pathspecs to limit scope
/// * `page_size_lines` - Lines per page (default ~800)
///
/// # Errors
///
/// Returns an error if:
/// - Not in a git repository
/// - Repository is bare
/// - No base ref can be found
/// - Diff generation fails
pub fn build_snapshot(
    start: &Path,
    mode: ReviewDiffMode,
    paths: &[String],
    page_size_lines: u32,
) -> Result<ReviewSnapshot, ToolError> {
    let repo = Repository::discover(start)
        .map_err(|e| ToolError::InvalidInput(format!("Not in a git repository: {e}")))?;

    let repo_root = repo
        .workdir()
        .ok_or_else(|| ToolError::InvalidInput("Bare repository".into()))?
        .canonicalize()
        .map_err(|e| ToolError::Internal(format!("Failed to canonicalize repo root: {e}")))?;

    let branch_slug = compute_branch_slug(&repo)?;
    let (base_ref_name, diff) = generate_diff(&repo, mode, paths)?;

    let stats = diff
        .stats()
        .map_err(|e| ToolError::Internal(format!("diff.stats: {e}")))?;
    let out_stats = DiffStats {
        files_changed: stats.files_changed() as u32,
        insertions: stats.insertions() as u32,
        deletions: stats.deletions() as u32,
    };

    let changed_files = collect_changed_files(&diff);
    let patch = diff_to_patch_string(&diff)?;
    let total_lines = patch.lines().count() as u32;
    let pages = paginate_patch(&patch, page_size_lines);

    Ok(ReviewSnapshot {
        repo_root,
        branch_slug,
        base_ref_name,
        pages,
        stats: out_stats,
        total_lines,
        page_size_lines,
        changed_files,
    })
}

/// Compute a URL-safe branch slug from HEAD.
fn compute_branch_slug(repo: &Repository) -> Result<String, ToolError> {
    let head = repo
        .head()
        .map_err(|e| ToolError::Internal(format!("Failed to get HEAD: {e}")))?;

    let name = head
        .shorthand()
        .unwrap_or("HEAD")
        .to_string()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase();

    Ok(name)
}

/// Find the first available base ref from fallback list.
fn find_base_ref(repo: &Repository) -> Result<String, ToolError> {
    for name in BASE_REF_FALLBACKS {
        if repo.revparse_single(name).is_ok() {
            return Ok((*name).to_string());
        }
    }
    Err(ToolError::InvalidInput(
        "Could not find base ref (tried origin/main, origin/master, main, master)".into(),
    ))
}

/// Generate a diff based on mode.
fn generate_diff<'a>(
    repo: &'a Repository,
    mode: ReviewDiffMode,
    paths: &[String],
) -> Result<(String, Diff<'a>), ToolError> {
    let mut opts = DiffOptions::new();
    opts.ignore_submodules(true);

    for path in paths {
        opts.pathspec(path);
    }

    match mode {
        ReviewDiffMode::Staged => {
            // Diff HEAD to index (staged changes only)
            let head_tree = repo
                .head()
                .and_then(|h| h.peel_to_tree())
                .map_err(|e| ToolError::Internal(format!("Failed to get HEAD tree: {e}")))?;

            let diff = repo
                .diff_tree_to_index(Some(&head_tree), None, Some(&mut opts))
                .map_err(|e| ToolError::Internal(format!("diff_tree_to_index: {e}")))?;

            Ok(("HEAD".to_string(), diff))
        }
        ReviewDiffMode::Default => {
            // Diff merge-base to working tree + index
            let base_ref_name = find_base_ref(repo)?;
            let base_obj = repo
                .revparse_single(&base_ref_name)
                .map_err(|e| ToolError::Internal(format!("revparse {base_ref_name}: {e}")))?;

            let head_obj = repo
                .head()
                .and_then(|h| h.peel_to_commit())
                .map_err(|e| ToolError::Internal(format!("HEAD peel: {e}")))?;

            // Find merge-base
            let merge_base = repo
                .merge_base(base_obj.id(), head_obj.id())
                .map_err(|e| ToolError::Internal(format!("merge_base: {e}")))?;

            let base_tree = repo
                .find_commit(merge_base)
                .and_then(|c| c.tree())
                .map_err(|e| ToolError::Internal(format!("merge_base tree: {e}")))?;

            let diff = repo
                .diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))
                .map_err(|e| ToolError::Internal(format!("diff_tree_to_workdir: {e}")))?;

            Ok((base_ref_name, diff))
        }
    }
}

/// Collect list of changed file paths from diff.
fn collect_changed_files(diff: &Diff<'_>) -> Vec<String> {
    let mut files = Vec::new();
    let _ = diff.foreach(
        &mut |delta, _| {
            if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                files.push(path.to_string_lossy().into_owned());
            }
            true
        },
        None,
        None,
        None,
    );
    files
}

/// Convert diff to patch string.
fn diff_to_patch_string(diff: &Diff<'_>) -> Result<String, ToolError> {
    let mut patch = String::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        if origin == '+' || origin == '-' || origin == ' ' {
            patch.push(origin);
        }
        patch.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
        true
    })
    .map_err(|e| ToolError::Internal(format!("diff.print: {e}")))?;

    Ok(patch)
}

/// Paginate a patch string into pages by file boundaries.
///
/// Strategy:
/// 1. Split patch by file (lines starting with "diff --git")
/// 2. Accumulate files into pages until `page_size_lines` is exceeded
/// 3. If a single file exceeds `page_size_lines`, keep it together with a warning
fn paginate_patch(patch: &str, page_size_lines: u32) -> Vec<DiffPage> {
    if patch.trim().is_empty() {
        return vec![];
    }

    let page_size = page_size_lines as usize;
    let mut pages = Vec::new();
    let mut current_page_lines: Vec<&str> = Vec::new();
    let mut current_page_files: Vec<String> = Vec::new();
    let mut current_file_lines: Vec<&str> = Vec::new();
    let mut current_file_name: Option<String> = None;

    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            // Flush current file to current page
            if !current_file_lines.is_empty() {
                let file_line_count = current_file_lines.len();

                // Check if adding this file would exceed page size
                if !current_page_lines.is_empty()
                    && current_page_lines.len() + file_line_count > page_size
                {
                    // Flush current page
                    pages.push(create_page(
                        pages.len() + 1,
                        &current_page_lines,
                        &current_page_files,
                        page_size,
                    ));
                    current_page_lines.clear();
                    current_page_files.clear();
                }

                // Add file to current page
                current_page_lines.extend(current_file_lines.iter());
                if let Some(name) = current_file_name.take() {
                    current_page_files.push(name);
                }
                current_file_lines.clear();
            }

            // Extract file name from "diff --git a/path b/path"
            current_file_name = extract_file_name(line);
        }
        current_file_lines.push(line);
    }

    // Flush remaining file
    if !current_file_lines.is_empty() {
        let file_line_count = current_file_lines.len();

        if !current_page_lines.is_empty() && current_page_lines.len() + file_line_count > page_size
        {
            pages.push(create_page(
                pages.len() + 1,
                &current_page_lines,
                &current_page_files,
                page_size,
            ));
            current_page_lines.clear();
            current_page_files.clear();
        }

        current_page_lines.extend(current_file_lines.iter());
        if let Some(name) = current_file_name.take() {
            current_page_files.push(name);
        }
    }

    // Flush remaining page
    if !current_page_lines.is_empty() {
        pages.push(create_page(
            pages.len() + 1,
            &current_page_lines,
            &current_page_files,
            page_size,
        ));
    }

    pages
}

/// Create a [`DiffPage`] from accumulated lines.
fn create_page(page_num: usize, lines: &[&str], files: &[String], page_size: usize) -> DiffPage {
    let line_count = lines.len();
    let oversized_warning = if line_count > page_size {
        Some(format!(
            "Page exceeds target size ({line_count} lines > {page_size}); contains large file(s) that cannot be split"
        ))
    } else {
        None
    };

    DiffPage {
        page: page_num as u32,
        content: lines.join("\n"),
        files_in_page: files.to_vec(),
        oversized_warning,
    }
}

/// Extract file name from a "diff --git" line.
fn extract_file_name(line: &str) -> Option<String> {
    // Format: "diff --git a/path/to/file b/path/to/file"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 4 {
        let b_path = parts[3];
        // Strip "b/" prefix if present
        let path = b_path.strip_prefix("b/").unwrap_or(b_path);
        Some(path.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_empty_patch() {
        let pages = paginate_patch("", 800);
        assert!(pages.is_empty());
    }

    #[test]
    fn paginate_single_file_under_limit() {
        let patch =
            "diff --git a/file.rs b/file.rs\n--- a/file.rs\n+++ b/file.rs\n+line1\n+line2\n";
        let pages = paginate_patch(patch, 800);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page, 1);
        assert!(pages[0].files_in_page.contains(&"file.rs".to_string()));
        assert!(pages[0].oversized_warning.is_none());
    }

    #[test]
    fn paginate_multiple_files() {
        let patch = concat!(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n+line1\n",
            "diff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n+line2\n"
        );
        let pages = paginate_patch(patch, 800);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].files_in_page.len(), 2);
    }

    #[test]
    fn paginate_splits_at_file_boundary() {
        // Create patch with 10 lines per file, page size of 5
        let patch = concat!(
            "diff --git a/a.rs b/a.rs\n1\n2\n3\n4\n5\n6\n7\n8\n9\n",
            "diff --git a/b.rs b/b.rs\n1\n2\n3\n4\n5\n6\n7\n8\n9\n"
        );
        let pages = paginate_patch(patch, 5);
        // Each file has 10 lines, which exceeds page size of 5
        // So each file should be on its own page
        assert_eq!(pages.len(), 2);
        assert!(pages[0].files_in_page.contains(&"a.rs".to_string()));
        assert!(pages[1].files_in_page.contains(&"b.rs".to_string()));
    }

    #[test]
    fn oversized_file_gets_warning() {
        let patch = "diff --git a/big.rs b/big.rs\n1\n2\n3\n4\n5\n6\n";
        let pages = paginate_patch(patch, 3);
        assert_eq!(pages.len(), 1);
        // 7 lines > page size of 3
        assert!(pages[0].oversized_warning.is_some());
    }

    #[test]
    fn extract_file_name_works() {
        let line = "diff --git a/src/lib.rs b/src/lib.rs";
        assert_eq!(extract_file_name(line), Some("src/lib.rs".to_string()));
    }

    #[test]
    fn base_ref_fallbacks_order() {
        assert_eq!(
            BASE_REF_FALLBACKS,
            &["origin/main", "origin/master", "main", "master"]
        );
    }
}
