//! End-to-end tests for indexed instant-grep behavior.

#![expect(clippy::unwrap_used)]

use coding_agent_tools::CodingAgentTools;
use coding_agent_tools::instant_grep::index::storage::resolve_index_paths;
use coding_agent_tools::types::OutputMode;
use git2::Repository;
use git2::Signature;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn commit_all(repo: &Repository, root: &Path, message: &str) -> String {
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        fs::write(&gitignore, ".thoughts-data\n").unwrap();
    }
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let oid = if let Some(parent) = parent.as_ref() {
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[parent])
            .unwrap()
    } else {
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
            .unwrap()
    };
    oid.to_string()
}

fn init_repo() -> (TempDir, Repository) {
    let tmp = TempDir::new().unwrap();
    let repo = Repository::init(tmp.path()).unwrap();
    (tmp, repo)
}

#[tokio::test]
async fn instant_grep_parity_supported_query_builds_index() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("base.txt"), "hello from base\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    let paths = resolve_index_paths(tmp.path()).unwrap();
    assert!(paths.current_generation_dir().unwrap().is_some());
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_git_backed_content_context() {
    let (tmp, repo) = init_repo();
    fs::write(
        tmp.path().join("context.txt"),
        "line1\nline2\nhello target\nline4\n",
    )
    .unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Content),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(1),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Content),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(1),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.warnings, instant.warnings);
    assert_eq!(grep.summary, instant.summary);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_git_backed_count_mode() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("count.txt"), "hello\nhello\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Count),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Count),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.summary, instant.summary);
    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.has_more, instant.has_more);
    assert!(!grep.has_more);
    assert!(!instant.has_more);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_git_backed_pagination() {
    let (tmp, repo) = init_repo();
    for i in 0..8 {
        fs::write(
            tmp.path().join(format!("file{i}.txt")),
            format!("hello from file {i}\n"),
        )
        .unwrap();
    }
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(3),
            Some(3),
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(3),
            Some(3),
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.has_more, instant.has_more);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_git_backed_multiline() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("multiline.txt"), "start\nfoo\nbar\nend\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "foo.*bar".to_string(),
            Some(root.clone()),
            Some(OutputMode::Content),
            None,
            None,
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "foo.*bar".to_string(),
            Some(root),
            Some(OutputMode::Content),
            None,
            None,
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.summary, instant.summary);
}

#[tokio::test]
async fn instant_grep_falls_back_for_case_insensitive_search() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("base.txt"), "Hello from base\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    let paths = resolve_index_paths(tmp.path()).unwrap();
    assert!(paths.current_generation_dir().unwrap().is_none());
}

#[tokio::test]
async fn instant_grep_sees_dirty_and_untracked_changes() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("base.txt"), "hello from base\n").unwrap();
    fs::write(tmp.path().join("tracked.txt"), "no match yet\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let _ = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    fs::write(tmp.path().join("tracked.txt"), "tracked hello now\n").unwrap();
    fs::write(tmp.path().join("new.txt"), "hello from untracked\n").unwrap();

    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert!(instant.lines.iter().any(|line| line.contains("base.txt")));
    assert!(
        instant
            .lines
            .iter()
            .any(|line| line.contains("tracked.txt"))
    );
    assert!(instant.lines.iter().any(|line| line.contains("new.txt")));
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_when_tracked_file_is_deleted_in_worktree() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("base.txt"), "hello from base\n").unwrap();
    fs::write(tmp.path().join("gone.txt"), "hello from gone\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    fs::remove_file(tmp.path().join("gone.txt")).unwrap();

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.warnings, instant.warnings);
}

#[tokio::test]
async fn instant_grep_rebuilds_on_head_change() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("base.txt"), "hello from base\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let _ = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let paths = resolve_index_paths(tmp.path()).unwrap();
    let first_generation = paths.current_generation_dir().unwrap().unwrap();

    fs::write(tmp.path().join("second.txt"), "hello from second commit\n").unwrap();
    commit_all(&repo, tmp.path(), "second");

    let _ = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let second_generation = paths.current_generation_dir().unwrap().unwrap();
    assert_ne!(first_generation, second_generation);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_in_non_git_directory() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("plain.txt"), "hello outside git\n").unwrap();

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_unplannable_pattern() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("one.txt"), "foo\n").unwrap();
    fs::write(tmp.path().join("two.txt"), "bar\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "foo|bar".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "foo|bar".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_short_pattern() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("short.txt"), "ab\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "ab".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "ab".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_broad_candidate_pattern() {
    let (tmp, repo) = init_repo();
    for i in 0..8 {
        fs::write(
            tmp.path().join(format!("file{i}.txt")),
            "common token here\n",
        )
        .unwrap();
    }
    fs::write(tmp.path().join("rare.txt"), "different\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "common".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "common".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_include_binary_fallback() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("text.txt"), "binary word in text\n").unwrap();
    fs::write(tmp.path().join("data.bin"), b"binary\0payload").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "binary".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(true),
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "binary".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(true),
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.warnings, instant.warnings);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_hidden_search_fallback() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("visible.txt"), "hello visible\n").unwrap();
    fs::write(tmp.path().join(".hidden.txt"), "hello hidden\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.warnings, instant.warnings);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_for_glob_fallbacks() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("one.rs"), "hello\n").unwrap();
    fs::write(tmp.path().join("two.txt"), "hello\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep_include = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            Some(vec!["*.rs".to_string()]),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant_include = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            Some(vec!["*.rs".to_string()]),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(grep_include.lines, instant_include.lines);

    let grep_ignore = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            Some(vec!["*.rs".to_string()]),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant_ignore = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            Some(vec!["*.rs".to_string()]),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(grep_ignore.lines, instant_ignore.lines);
}

#[tokio::test]
async fn instant_grep_matches_scan_grep_when_index_build_fails() {
    let (tmp, repo) = init_repo();
    fs::write(tmp.path().join("base.txt"), "hello from base\n").unwrap();
    commit_all(&repo, tmp.path(), "init");

    let paths = resolve_index_paths(tmp.path()).unwrap();
    fs::create_dir_all(paths.root_dir.parent().unwrap()).unwrap();
    fs::write(&paths.root_dir, "not a directory").unwrap();

    let tools = CodingAgentTools::new();
    let root = tmp.path().to_string_lossy().to_string();

    let grep = tools
        .search_grep(
            "hello".to_string(),
            Some(root.clone()),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let instant = tools
        .search_instant_grep(
            "hello".to_string(),
            Some(root),
            Some(OutputMode::Files),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(grep.lines, instant.lines);
    assert_eq!(grep.warnings, instant.warnings);
}
