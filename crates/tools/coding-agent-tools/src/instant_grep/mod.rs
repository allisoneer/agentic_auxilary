//! Additive instant-grep implementation.
//!
//! This module implements the additive `cli_instant_grep` MVP: indexed-first
//! execution for the supported default profile, with correctness-first fallback
//! to the existing scan-based grep path for unsupported, unsafe, or unhelpful
//! cases.

pub mod decision;
pub mod grams;
pub mod index;
pub mod overlay;
pub mod planner;

use crate::grep::GrepConfig;
use crate::grep::{self};
use crate::types::GrepOutput;
use agentic_tools_core::ToolError;
use git2::Repository;
use std::path::Path;
use std::path::PathBuf;
use thoughts_tool::git::utils::find_repo_root;

pub const INDEX_FORMAT_VERSION: u32 = 1;

/// Run the instant-grep entrypoint.
///
/// Phase 3 behavior: use indexed execution for the supported default profile and
/// fall back to the scan path otherwise.
///
/// MVP fallback boundaries:
/// - non-git roots
/// - unsupported profile options (hidden, binary, case-insensitive, extra globs)
/// - unplannable or unhelpful queries
/// - index build/open failures
pub fn run(cfg: GrepConfig) -> Result<GrepOutput, ToolError> {
    let root_path = Path::new(&cfg.root);
    let Ok(repo_root) = find_repo_root(root_path) else {
        return grep::run(cfg);
    };

    if decision::decide_mode(&cfg, &repo_root) != decision::ExecutionMode::Indexed {
        return grep::run(cfg);
    }

    let Ok(repo) = Repository::open(&repo_root) else {
        return grep::run(cfg);
    };
    let head_oid = match repo.head().ok().and_then(|head| head.target()) {
        Some(oid) => oid.to_string(),
        None => return grep::run(cfg),
    };

    let Ok(index) = index::reader::open_or_build(&repo_root, &head_oid) else {
        return grep::run(cfg);
    };

    let Ok(Some(plan)) = planner::plan_query(&index, &cfg.pattern) else {
        return grep::run(cfg);
    };

    let mut paths: Vec<PathBuf> = plan
        .candidate_doc_ids
        .into_iter()
        .map(|doc_id| repo_root.join(index.doc_path(doc_id)))
        .filter(|path| path_matches_query_root(path, root_path))
        .collect();

    if let Ok(overlay) = overlay::overlay_paths(&repo_root) {
        paths.extend(
            overlay
                .dirty_tracked
                .into_iter()
                .chain(overlay.untracked)
                .filter(|path| path_matches_query_root(path, root_path)),
        );
    }

    paths.sort();
    paths.dedup();

    grep::run_on_paths(cfg, paths, vec![])
}

fn path_matches_query_root(path: &Path, query_root: &Path) -> bool {
    if query_root.is_file() {
        path == query_root
    } else {
        path.starts_with(query_root)
    }
}
