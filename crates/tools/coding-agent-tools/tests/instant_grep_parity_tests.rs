//! Parity tests for the additive `cli_instant_grep` surface.

#![expect(clippy::unwrap_used)]

use coding_agent_tools::CodingAgentTools;
use coding_agent_tools::types::{GrepOutput, OutputMode};
use std::fs;
use tempfile::TempDir;

fn setup_test_dir() -> TempDir {
    let tmp = TempDir::new().unwrap();

    fs::write(tmp.path().join("hello.txt"), "Hello World\nfoo bar\nbaz").unwrap();
    fs::write(
        tmp.path().join("code.rs"),
        "fn main() {\n    println!(\"Hello\");\n}\n",
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join("subdir")).unwrap();
    fs::write(
        tmp.path().join("subdir/nested.txt"),
        "nested content\nwith hello inside",
    )
    .unwrap();

    tmp
}

async fn run_both(
    tools: &CodingAgentTools,
    pattern: &str,
    path: String,
    mode: Option<OutputMode>,
    multiline: Option<bool>,
    context: Option<u32>,
    head_limit: Option<usize>,
    offset: Option<usize>,
) -> (GrepOutput, GrepOutput) {
    let grep = tools
        .search_grep(
            pattern.to_string(),
            Some(path.clone()),
            mode,
            None,
            None,
            None,
            None,
            multiline,
            None,
            context,
            None,
            None,
            None,
            head_limit,
            offset,
        )
        .await
        .unwrap();

    let instant = tools
        .search_instant_grep(
            pattern.to_string(),
            Some(path),
            mode,
            None,
            None,
            None,
            None,
            multiline,
            None,
            context,
            None,
            None,
            None,
            head_limit,
            offset,
        )
        .await
        .unwrap();

    (grep, instant)
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_files_mode() {
    let tmp = setup_test_dir();
    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let (grep, instant) = run_both(
        &tools,
        "hello",
        root,
        Some(OutputMode::Files),
        Some(false),
        None,
        Some(200),
        Some(0),
    )
    .await;

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.has_more, instant.has_more);
    assert_eq!(grep.warnings, instant.warnings);
    assert_eq!(grep.summary, instant.summary);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_content_context() {
    let tmp = setup_test_dir();
    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let (grep, instant) = run_both(
        &tools,
        "hello",
        root,
        Some(OutputMode::Content),
        Some(false),
        Some(1),
        Some(200),
        Some(0),
    )
    .await;

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.has_more, instant.has_more);
    assert_eq!(grep.warnings, instant.warnings);
    assert_eq!(grep.summary, instant.summary);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_count_mode() {
    let tmp = setup_test_dir();
    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let (grep, instant) = run_both(
        &tools,
        "hello",
        root,
        Some(OutputMode::Count),
        Some(false),
        None,
        Some(200),
        Some(0),
    )
    .await;

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.has_more, instant.has_more);
    assert_eq!(grep.warnings, instant.warnings);
    assert_eq!(grep.summary, instant.summary);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_pagination() {
    let tmp = TempDir::new().unwrap();
    for i in 0..8 {
        fs::write(
            tmp.path().join(format!("file{i}.txt")),
            format!("content {i}\nmatch here"),
        )
        .unwrap();
    }

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let (grep, instant) = run_both(
        &tools,
        "match",
        root,
        Some(OutputMode::Files),
        Some(false),
        None,
        Some(3),
        Some(3),
    )
    .await;

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.has_more, instant.has_more);
    assert_eq!(grep.warnings, instant.warnings);
    assert_eq!(grep.summary, instant.summary);
}
