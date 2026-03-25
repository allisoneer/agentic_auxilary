//! Execution mode selection for instant-grep.

use crate::grep::GrepConfig;
use std::path::Path;
use thoughts_tool::git::utils::is_git_repo;

/// Instant-grep execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Indexed,
    ScanFallback,
}

/// Return the Phase 3 execution mode for the current request.
///
/// MVP note: indexed execution is intentionally limited to the default profile
/// first. Unsupported cases fall back to the existing scan path to preserve
/// correctness while the indexed path matures.
pub fn decide_mode(cfg: &GrepConfig, repo_root: &Path) -> ExecutionMode {
    if !is_git_repo(repo_root) {
        return ExecutionMode::ScanFallback;
    }

    if cfg.include_hidden
        || cfg.include_binary
        || cfg.case_insensitive
        || !cfg.include_globs.is_empty()
        || !cfg.ignore_globs.is_empty()
    {
        return ExecutionMode::ScanFallback;
    }

    ExecutionMode::Indexed
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::OutputMode;
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    fn test_cfg(root: &str) -> GrepConfig {
        GrepConfig {
            root: root.to_string(),
            pattern: "hello".to_string(),
            mode: OutputMode::Files,
            include_globs: vec![],
            ignore_globs: vec![],
            include_hidden: false,
            case_insensitive: false,
            multiline: false,
            line_numbers: true,
            context: None,
            context_before: None,
            context_after: None,
            include_binary: false,
            head_limit: 200,
            offset: 0,
        }
    }

    fn init_git_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let sig = Signature::now("Test", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        tmp
    }

    #[test]
    fn indexed_mode_requires_git_repo_and_default_profile() {
        let tmp = init_git_repo();
        let cfg = test_cfg(&tmp.path().to_string_lossy());
        assert_eq!(decide_mode(&cfg, tmp.path()), ExecutionMode::Indexed);
    }

    #[test]
    fn hidden_search_forces_scan_fallback() {
        let tmp = init_git_repo();
        let mut cfg = test_cfg(&tmp.path().to_string_lossy());
        cfg.include_hidden = true;
        assert_eq!(decide_mode(&cfg, tmp.path()), ExecutionMode::ScanFallback);
    }

    #[test]
    fn binary_search_forces_scan_fallback() {
        let tmp = init_git_repo();
        let mut cfg = test_cfg(&tmp.path().to_string_lossy());
        cfg.include_binary = true;
        assert_eq!(decide_mode(&cfg, tmp.path()), ExecutionMode::ScanFallback);
    }

    #[test]
    fn include_globs_force_scan_fallback() {
        let tmp = init_git_repo();
        let mut cfg = test_cfg(&tmp.path().to_string_lossy());
        cfg.include_globs.push("*.rs".to_string());
        assert_eq!(decide_mode(&cfg, tmp.path()), ExecutionMode::ScanFallback);
    }

    #[test]
    fn ignore_globs_force_scan_fallback() {
        let tmp = init_git_repo();
        let mut cfg = test_cfg(&tmp.path().to_string_lossy());
        cfg.ignore_globs.push("*.rs".to_string());
        assert_eq!(decide_mode(&cfg, tmp.path()), ExecutionMode::ScanFallback);
    }

    #[test]
    fn case_insensitive_forces_scan_fallback() {
        let tmp = init_git_repo();
        let mut cfg = test_cfg(&tmp.path().to_string_lossy());
        cfg.case_insensitive = true;
        assert_eq!(decide_mode(&cfg, tmp.path()), ExecutionMode::ScanFallback);
    }

    #[test]
    fn non_git_repo_forces_scan_fallback() {
        let tmp = TempDir::new().unwrap();
        let cfg = test_cfg(&tmp.path().to_string_lossy());
        assert_eq!(decide_mode(&cfg, tmp.path()), ExecutionMode::ScanFallback);
    }
}
