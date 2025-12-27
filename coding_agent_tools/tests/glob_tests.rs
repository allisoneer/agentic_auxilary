//! Integration tests for search_glob.

use coding_agent_tools::types::SortOrder;
use filetime::{FileTime, set_file_mtime};
use std::fs;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// Helper to create a GlobConfig and run glob.
fn run_glob(
    root: &str,
    pattern: &str,
    ignore_globs: Vec<String>,
    include_hidden: bool,
    sort: SortOrder,
    head_limit: usize,
    offset: usize,
) -> Result<coding_agent_tools::types::GlobOutput, universal_tool_core::prelude::ToolError> {
    let cfg = coding_agent_tools::glob::GlobConfig {
        root: root.to_string(),
        pattern: pattern.to_string(),
        ignore_globs,
        include_hidden,
        sort,
        head_limit,
        offset,
    };
    coding_agent_tools::glob::run(cfg)
}

/// Create a temp directory with test files.
fn setup_test_dir() -> TempDir {
    let tmp = TempDir::new().unwrap();

    // Create files
    fs::write(tmp.path().join("alpha.txt"), "content").unwrap();
    fs::write(tmp.path().join("beta.rs"), "content").unwrap();
    fs::write(tmp.path().join("gamma.py"), "content").unwrap();

    // Create subdirectory with files
    fs::create_dir(tmp.path().join("subdir")).unwrap();
    fs::write(tmp.path().join("subdir/delta.txt"), "content").unwrap();
    fs::write(tmp.path().join("subdir/epsilon.rs"), "content").unwrap();

    // Create nested subdirectory
    fs::create_dir(tmp.path().join("subdir/nested")).unwrap();
    fs::write(tmp.path().join("subdir/nested/zeta.txt"), "content").unwrap();

    // Create hidden file
    fs::write(tmp.path().join(".hidden"), "content").unwrap();

    tmp
}

#[test]
fn test_glob_basic_pattern() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(&root, "*.txt", vec![], false, SortOrder::Name, 500, 0).unwrap();

    // Should find .txt files in root (not recursive by default for this pattern)
    assert!(
        result.entries.iter().any(|p| p == "alpha.txt"),
        "Should find alpha.txt"
    );
}

#[test]
fn test_glob_recursive_pattern() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(&root, "**/*.txt", vec![], false, SortOrder::Name, 500, 0).unwrap();

    // Should find all .txt files recursively
    assert!(
        result.entries.iter().any(|p| p == "alpha.txt"),
        "Should find alpha.txt"
    );
    assert!(
        result.entries.iter().any(|p| p.contains("delta.txt")),
        "Should find subdir/delta.txt"
    );
    assert!(
        result.entries.iter().any(|p| p.contains("zeta.txt")),
        "Should find subdir/nested/zeta.txt"
    );
}

#[test]
fn test_glob_sort_by_name() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(&root, "*.txt", vec![], false, SortOrder::Name, 500, 0).unwrap();

    // Results should be alphabetically sorted
    let sorted: Vec<String> = {
        let mut v = result.entries.clone();
        v.sort_by_key(|a| a.to_lowercase());
        v
    };
    assert_eq!(result.entries, sorted, "Should be alphabetically sorted");
}

#[test]
fn test_glob_sort_by_mtime() {
    let tmp = TempDir::new().unwrap();

    // Create files with different mtimes
    let now = SystemTime::now();

    // Create oldest file first
    fs::write(tmp.path().join("old.txt"), "old").unwrap();
    let old_time = FileTime::from_system_time(now - Duration::from_secs(3600));
    set_file_mtime(tmp.path().join("old.txt"), old_time).unwrap();

    // Create middle file
    fs::write(tmp.path().join("mid.txt"), "mid").unwrap();
    let mid_time = FileTime::from_system_time(now - Duration::from_secs(1800));
    set_file_mtime(tmp.path().join("mid.txt"), mid_time).unwrap();

    // Create newest file
    fs::write(tmp.path().join("new.txt"), "new").unwrap();
    let new_time = FileTime::from_system_time(now);
    set_file_mtime(tmp.path().join("new.txt"), new_time).unwrap();

    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(&root, "*.txt", vec![], false, SortOrder::Mtime, 500, 0).unwrap();

    // Should be sorted newest first
    assert_eq!(result.entries.len(), 3);
    assert_eq!(result.entries[0], "new.txt", "Newest should be first");
    assert_eq!(result.entries[1], "mid.txt", "Middle should be second");
    assert_eq!(result.entries[2], "old.txt", "Oldest should be last");
}

#[test]
fn test_glob_ignore_patterns() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(
        &root,
        "**/*",
        vec!["*.rs".to_string()], // Exclude .rs files
        false,
        SortOrder::Name,
        500,
        0,
    )
    .unwrap();

    // Should not find .rs files
    for path in &result.entries {
        assert!(
            !path.ends_with(".rs"),
            "Should not match .rs files: {}",
            path
        );
    }
}

#[test]
fn test_glob_include_hidden() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    // Without include_hidden
    let result_no_hidden = run_glob(
        &root,
        "*",
        vec![],
        false, // don't include hidden
        SortOrder::Name,
        500,
        0,
    )
    .unwrap();

    // With include_hidden
    let result_with_hidden = run_glob(
        &root,
        "*",
        vec![],
        true, // include hidden
        SortOrder::Name,
        500,
        0,
    )
    .unwrap();

    // Hidden file should only appear when include_hidden is true
    let has_hidden_no = result_no_hidden.entries.iter().any(|p| p.starts_with('.'));
    let has_hidden_yes = result_with_hidden
        .entries
        .iter()
        .any(|p| p.starts_with('.'));

    assert!(!has_hidden_no, "Hidden file should not appear without flag");
    assert!(has_hidden_yes, "Hidden file should appear with flag");
}

#[test]
fn test_glob_pagination() {
    let tmp = TempDir::new().unwrap();

    // Create multiple files
    for i in 0..10 {
        fs::write(tmp.path().join(format!("file{:02}.txt", i)), "content").unwrap();
    }

    let root = tmp.path().to_string_lossy().to_string();

    // Get first 3 results
    let result1 = run_glob(
        &root,
        "*.txt",
        vec![],
        false,
        SortOrder::Name,
        3, // head_limit
        0, // offset
    )
    .unwrap();

    assert_eq!(result1.entries.len(), 3);
    assert!(result1.has_more, "Should have more results");

    // Get next 3 results
    let result2 = run_glob(
        &root,
        "*.txt",
        vec![],
        false,
        SortOrder::Name,
        3,
        3, // offset
    )
    .unwrap();

    assert_eq!(result2.entries.len(), 3);
    assert!(result2.has_more);

    // Results should be different (no overlap)
    for entry in &result1.entries {
        assert!(
            !result2.entries.contains(entry),
            "Should not have overlapping results"
        );
    }
}

#[test]
fn test_glob_invalid_pattern() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(
        &root,
        "[invalid", // Invalid glob pattern
        vec![],
        false,
        SortOrder::Name,
        500,
        0,
    );

    assert!(result.is_err(), "Should fail with invalid pattern");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Invalid glob pattern"),
        "Error should mention invalid pattern"
    );
}

#[test]
fn test_glob_no_matches() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(
        &root,
        "*.nonexistent",
        vec![],
        false,
        SortOrder::Name,
        500,
        0,
    )
    .unwrap();

    assert!(result.entries.is_empty(), "Should have no matches");
    assert!(!result.has_more);
}

#[test]
fn test_glob_nonexistent_path() {
    let result = run_glob(
        "/nonexistent/path/12345",
        "*.txt",
        vec![],
        false,
        SortOrder::Name,
        500,
        0,
    );

    assert!(result.is_err(), "Should fail with nonexistent path");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("does not exist"),
        "Error should mention path does not exist"
    );
}

#[test]
fn test_glob_matches_directories() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(&root, "**/*", vec![], false, SortOrder::Name, 500, 0).unwrap();

    // Should include both files and directories
    assert!(
        result
            .entries
            .iter()
            .any(|p| p == "subdir" || p.ends_with("subdir")),
        "Should match directories"
    );
}

#[test]
fn test_glob_specific_extension() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(&root, "**/*.rs", vec![], false, SortOrder::Name, 500, 0).unwrap();

    // Should only find .rs files
    assert!(!result.entries.is_empty(), "Should find .rs files");
    for path in &result.entries {
        assert!(
            path.ends_with(".rs"),
            "Should only match .rs files: {}",
            path
        );
    }
}

#[test]
fn test_glob_builtin_ignores() {
    let tmp = TempDir::new().unwrap();

    // Create a node_modules directory (should be ignored by default)
    fs::create_dir(tmp.path().join("node_modules")).unwrap();
    fs::write(tmp.path().join("node_modules/package.json"), "{}").unwrap();

    // Create a regular file
    fs::write(tmp.path().join("app.js"), "content").unwrap();

    let root = tmp.path().to_string_lossy().to_string();

    let result = run_glob(&root, "**/*", vec![], false, SortOrder::Name, 500, 0).unwrap();

    // Should not include node_modules
    for path in &result.entries {
        assert!(
            !path.contains("node_modules"),
            "Should not match node_modules: {}",
            path
        );
    }
    assert!(
        result.entries.iter().any(|p| p == "app.js"),
        "Should find app.js"
    );
}
