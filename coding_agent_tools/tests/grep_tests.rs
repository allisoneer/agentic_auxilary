//! Integration tests for search_grep.

use coding_agent_tools::types::OutputMode;
use std::fs;
use tempfile::TempDir;

/// Helper to create a GrepConfig and run grep.
fn run_grep(
    root: &str,
    pattern: &str,
    mode: OutputMode,
    include_globs: Vec<String>,
    ignore_globs: Vec<String>,
    include_hidden: bool,
    case_insensitive: bool,
    multiline: bool,
    line_numbers: bool,
    context: Option<u32>,
    context_before: Option<u32>,
    context_after: Option<u32>,
    include_binary: bool,
    head_limit: usize,
    offset: usize,
) -> Result<coding_agent_tools::types::GrepOutput, universal_tool_core::prelude::ToolError> {
    let cfg = coding_agent_tools::grep::GrepConfig {
        root: root.to_string(),
        pattern: pattern.to_string(),
        mode,
        include_globs,
        ignore_globs,
        include_hidden,
        case_insensitive,
        multiline,
        line_numbers,
        context,
        context_before,
        context_after,
        include_binary,
        head_limit,
        offset,
    };
    coding_agent_tools::grep::run(cfg)
}

/// Create a temp directory with test files.
fn setup_test_dir() -> TempDir {
    let tmp = TempDir::new().unwrap();

    // Create files with content
    fs::write(tmp.path().join("hello.txt"), "Hello World\nfoo bar\nbaz").unwrap();
    fs::write(
        tmp.path().join("code.rs"),
        "fn main() {\n    println!(\"Hello\");\n}\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("test.py"),
        "def hello():\n    print('world')\n",
    )
    .unwrap();

    // Create a subdirectory with files
    fs::create_dir(tmp.path().join("subdir")).unwrap();
    fs::write(
        tmp.path().join("subdir/nested.txt"),
        "nested content\nwith hello inside",
    )
    .unwrap();

    // Create a hidden file
    fs::write(tmp.path().join(".hidden"), "hidden hello content").unwrap();

    tmp
}

#[test]
fn test_grep_files_mode_basic() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "hello",
        OutputMode::Files,
        vec![],
        vec![],
        false,
        true, // case_insensitive to match "Hello"
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should find files containing "hello" (case insensitive)
    assert!(!result.lines.is_empty(), "Should find matches");
    assert!(
        result.lines.iter().any(|p| p.contains("hello.txt")),
        "Should find hello.txt"
    );
    assert!(
        result.lines.iter().any(|p| p.contains("code.rs")),
        "Should find code.rs (contains Hello)"
    );
}

#[test]
fn test_grep_content_mode_with_line_numbers() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "hello",
        OutputMode::Content,
        vec![],
        vec![],
        false,
        true,
        false,
        true, // line_numbers
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should contain file:line: content format
    assert!(!result.lines.is_empty());
    // Lines should contain line numbers
    for line in &result.lines {
        assert!(
            line.contains(':'),
            "Line should be in path:line: format: {}",
            line
        );
    }
}

#[test]
fn test_grep_count_mode() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "hello",
        OutputMode::Count,
        vec![],
        vec![],
        false,
        true,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should have summary with count
    assert!(result.summary.is_some());
    assert!(result.summary.unwrap().contains("Total matches:"));
    assert!(result.lines.is_empty()); // Count mode doesn't return lines
}

#[test]
fn test_grep_include_globs() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "hello",
        OutputMode::Files,
        vec!["*.txt".to_string()], // Only .txt files
        vec![],
        false,
        true,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should only find .txt files
    for path in &result.lines {
        assert!(
            path.ends_with(".txt"),
            "Should only match .txt files: {}",
            path
        );
    }
}

#[test]
fn test_grep_ignore_globs() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "hello",
        OutputMode::Files,
        vec![],
        vec!["*.rs".to_string()], // Exclude .rs files
        false,
        true,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should not find .rs files
    for path in &result.lines {
        assert!(
            !path.ends_with(".rs"),
            "Should not match .rs files: {}",
            path
        );
    }
}

#[test]
fn test_grep_include_hidden() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    // Without include_hidden
    let result_no_hidden = run_grep(
        &root,
        "hello",
        OutputMode::Files,
        vec![],
        vec![],
        false, // don't include hidden
        true,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // With include_hidden
    let result_with_hidden = run_grep(
        &root,
        "hello",
        OutputMode::Files,
        vec![],
        vec![],
        true, // include hidden
        true,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Hidden file should only appear when include_hidden is true
    let has_hidden_no = result_no_hidden.lines.iter().any(|p| p.contains(".hidden"));
    let has_hidden_yes = result_with_hidden
        .lines
        .iter()
        .any(|p| p.contains(".hidden"));

    assert!(!has_hidden_no, "Hidden file should not appear without flag");
    assert!(has_hidden_yes, "Hidden file should appear with flag");
}

#[test]
fn test_grep_case_sensitive() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    // Case sensitive search for "hello" (lowercase)
    let result = run_grep(
        &root,
        "hello",
        OutputMode::Files,
        vec![],
        vec![],
        false,
        false, // case sensitive
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should only find files with lowercase "hello"
    // hello.txt has "Hello" (capital H), so it shouldn't match
    // subdir/nested.txt has "hello" (lowercase), so it should match
    assert!(
        result.lines.iter().any(|p| p.contains("nested.txt")),
        "Should find nested.txt with lowercase hello"
    );
}

#[test]
fn test_grep_multiline() {
    let tmp = TempDir::new().unwrap();

    // Create a file with multiline content
    fs::write(tmp.path().join("multiline.txt"), "start\nfoo\nbar\nend").unwrap();

    let root = tmp.path().to_string_lossy().to_string();

    // Search for pattern that spans lines
    let result = run_grep(
        &root,
        "foo.*bar",
        OutputMode::Content,
        vec![],
        vec![],
        false,
        false,
        true, // multiline mode
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should find the multiline match
    assert!(!result.lines.is_empty(), "Should find multiline match");
}

#[test]
fn test_grep_context_lines() {
    let tmp = TempDir::new().unwrap();

    // Create a file with numbered lines
    fs::write(
        tmp.path().join("context.txt"),
        "line1\nline2\nTARGET\nline4\nline5\n",
    )
    .unwrap();

    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "TARGET",
        OutputMode::Content,
        vec![],
        vec![],
        false,
        false,
        false,
        true,
        Some(1), // 1 line of context before and after
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    // Should include context lines
    assert!(result.lines.len() >= 3, "Should include context lines");
    // Check that context lines are present
    let content: String = result.lines.join("\n");
    assert!(content.contains("line2"), "Should have context before");
    assert!(content.contains("TARGET"), "Should have match");
    assert!(content.contains("line4"), "Should have context after");
}

#[test]
fn test_grep_pagination() {
    let tmp = TempDir::new().unwrap();

    // Create multiple files with matches
    for i in 0..10 {
        fs::write(
            tmp.path().join(format!("file{}.txt", i)),
            format!("content {}\nmatch here", i),
        )
        .unwrap();
    }

    let root = tmp.path().to_string_lossy().to_string();

    // Get first 3 results
    let result1 = run_grep(
        &root,
        "match",
        OutputMode::Files,
        vec![],
        vec![],
        false,
        false,
        false,
        true,
        None,
        None,
        None,
        false,
        3, // head_limit
        0, // offset
    )
    .unwrap();

    assert_eq!(result1.lines.len(), 3);
    assert!(result1.has_more, "Should have more results");

    // Get next 3 results
    let result2 = run_grep(
        &root,
        "match",
        OutputMode::Files,
        vec![],
        vec![],
        false,
        false,
        false,
        true,
        None,
        None,
        None,
        false,
        3,
        3, // offset
    )
    .unwrap();

    assert_eq!(result2.lines.len(), 3);
    assert!(result2.has_more);
}

#[test]
fn test_grep_binary_file_skip() {
    let tmp = TempDir::new().unwrap();

    // Create a binary file (contains NUL byte)
    let binary_content = b"binary\x00content";
    fs::write(tmp.path().join("binary.bin"), binary_content).unwrap();

    // Create a text file
    fs::write(tmp.path().join("text.txt"), "binary text").unwrap();

    let root = tmp.path().to_string_lossy().to_string();

    // Search without include_binary
    let result = run_grep(
        &root,
        "binary",
        OutputMode::Files,
        vec![],
        vec![],
        false,
        false,
        false,
        true,
        None,
        None,
        None,
        false, // don't include binary
        200,
        0,
    )
    .unwrap();

    // Should only find text file, not binary
    assert!(
        result.lines.iter().any(|p| p.contains("text.txt")),
        "Should find text file"
    );
    assert!(
        !result.lines.iter().any(|p| p.contains("binary.bin")),
        "Should not find binary file"
    );
    // Should have warning about skipped binary
    assert!(
        result.warnings.iter().any(|w| w.contains("binary")),
        "Should warn about skipped binary"
    );
}

#[test]
fn test_grep_invalid_regex() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "[invalid", // Invalid regex
        OutputMode::Files,
        vec![],
        vec![],
        false,
        false,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    );

    assert!(result.is_err(), "Should fail with invalid regex");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Invalid regex"),
        "Error should mention invalid regex"
    );
}

#[test]
fn test_grep_no_matches() {
    let tmp = setup_test_dir();
    let root = tmp.path().to_string_lossy().to_string();

    let result = run_grep(
        &root,
        "xyznonexistent123",
        OutputMode::Files,
        vec![],
        vec![],
        false,
        false,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    assert!(result.lines.is_empty(), "Should have no matches");
    assert!(!result.has_more);
}

#[test]
fn test_grep_single_file() {
    let tmp = setup_test_dir();
    let file_path = tmp.path().join("hello.txt").to_string_lossy().to_string();

    let result = run_grep(
        &file_path,
        "Hello",
        OutputMode::Content,
        vec![],
        vec![],
        false,
        false,
        false,
        true,
        None,
        None,
        None,
        false,
        200,
        0,
    )
    .unwrap();

    assert!(!result.lines.is_empty(), "Should find match in single file");
}
