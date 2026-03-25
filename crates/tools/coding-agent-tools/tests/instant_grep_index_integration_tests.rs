//! Integration tests for instant-grep index build/open behavior.

#![expect(clippy::unwrap_used)]

use coding_agent_tools::instant_grep::grams::all_grams;
use coding_agent_tools::instant_grep::index::builder::build_index_for_head;
use coding_agent_tools::instant_grep::index::reader::open_or_build;
use coding_agent_tools::instant_grep::index::storage::resolve_index_paths;
use git2::{Repository, Signature};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn commit_file(repo: &Repository, root: &Path, rel: &str, content: &str, message: &str) -> String {
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        fs::write(&gitignore, ".thoughts-data\n").unwrap();
    }

    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();

    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let oid = if let Some(parent) = parent.as_ref() {
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[parent])
            .unwrap()
    } else {
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
            .unwrap()
    };
    oid.to_string()
}

#[test]
fn build_index_for_head_writes_generation_files() {
    let tmp = TempDir::new().unwrap();
    let repo = Repository::init(tmp.path()).unwrap();
    let head = commit_file(&repo, tmp.path(), "src/main.rs", "fn main() {}\n", "init");

    let generation = build_index_for_head(tmp.path(), &head).unwrap();
    assert!(generation.dir.exists());
    assert!(generation.meta_json.exists());
    assert!(generation.docs_bin.exists());
    assert!(generation.lookup_bin.exists());
    assert!(generation.postings_bin.exists());
}

#[test]
fn open_or_build_reads_back_docs_and_postings() {
    let tmp = TempDir::new().unwrap();
    let repo = Repository::init(tmp.path()).unwrap();
    let head = commit_file(&repo, tmp.path(), "src/main.rs", "fn main() {}\n", "init");

    let index = open_or_build(tmp.path(), &head).unwrap();
    assert_eq!(index.meta.head_oid, head);
    assert_eq!(index.meta.doc_count, 1);
    assert_eq!(index.doc_path(0), "src/main.rs");

    let first_gram = all_grams(b"fn main() {}\n").next().unwrap();
    let postings = index.postings(first_gram).unwrap();
    assert_eq!(postings, vec![0]);
}

#[test]
fn open_or_build_reuses_generation_for_same_head() {
    let tmp = TempDir::new().unwrap();
    let repo = Repository::init(tmp.path()).unwrap();
    let head = commit_file(
        &repo,
        tmp.path(),
        "src/lib.rs",
        "pub fn demo() {}\n",
        "init",
    );

    let _first = open_or_build(tmp.path(), &head).unwrap();
    let paths = resolve_index_paths(tmp.path()).unwrap();
    let first_generation = paths.current_generation_dir().unwrap().unwrap();

    let _second = open_or_build(tmp.path(), &head).unwrap();
    let second_generation = paths.current_generation_dir().unwrap().unwrap();

    assert_eq!(first_generation, second_generation);
}
