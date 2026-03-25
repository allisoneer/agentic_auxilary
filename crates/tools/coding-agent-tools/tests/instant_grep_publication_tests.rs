//! Publication safety tests for instant-grep index generations.

#![expect(clippy::unwrap_used)]

use coding_agent_tools::instant_grep::index::builder::build_index_for_head;
use coding_agent_tools::instant_grep::index::reader::open_or_build;
use git2::{Repository, Signature};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
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

#[test]
fn readers_never_observe_partially_published_index() {
    let tmp = TempDir::new().unwrap();
    let repo = Repository::init(tmp.path()).unwrap();
    fs::write(tmp.path().join("base.txt"), "hello from base\n").unwrap();
    let head = commit_all(&repo, tmp.path(), "init");

    let repo_root = Arc::new(tmp.path().to_path_buf());
    let head = Arc::new(head);

    thread::scope(|scope| {
        let root_for_builder = Arc::clone(&repo_root);
        let head_for_builder = Arc::clone(&head);
        let builder = scope.spawn(move || {
            for _ in 0..20 {
                let generation =
                    build_index_for_head(&root_for_builder, &head_for_builder).unwrap();
                assert!(generation.meta_json.exists());
                assert!(generation.lookup_bin.exists());
                assert!(generation.postings_bin.exists());
            }
        });

        let root_for_reader = Arc::clone(&repo_root);
        let head_for_reader = Arc::clone(&head);
        let reader = scope.spawn(move || {
            for _ in 0..20 {
                let index = open_or_build(&root_for_reader, &head_for_reader).unwrap();
                assert_eq!(index.meta.head_oid, *head_for_reader);
            }
        });

        builder.join().unwrap();
        reader.join().unwrap();
    });
}
