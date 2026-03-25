//! Immutable instant-grep index builder.

use super::format::{IndexMeta, LookupEntry};
use super::storage::{GenerationPaths, resolve_index_paths};
use crate::instant_grep::grams::{GramKey, all_grams};
use crate::walker;
use anyhow::{Context, Result, bail};
use git2::{ObjectType, Oid, Repository, Tree};
use globset::GlobSet;
use ignore::WalkBuilder;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use thoughts_tool::utils::locks::FileLock;

fn is_hidden_path(rel_path: &str) -> bool {
    rel_path.split('/').any(|part| part.starts_with('.'))
}

fn is_binary_bytes(bytes: &[u8]) -> bool {
    let len = bytes.len().min(8192);
    bytes[..len].contains(&0)
}

fn should_index_path(ignore_gs: &GlobSet, rel_path: &str) -> bool {
    !rel_path.is_empty() && !is_hidden_path(rel_path) && !ignore_gs.is_match(rel_path)
}

fn walk_tree(
    repo: &Repository,
    tree: &Tree<'_>,
    prefix: &Path,
    ignore_gs: &GlobSet,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<()> {
    for entry in tree {
        let Some(name) = entry.name() else {
            continue;
        };

        let rel_path = if prefix.as_os_str().is_empty() {
            PathBuf::from(name)
        } else {
            prefix.join(name)
        };
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");

        match entry.kind() {
            Some(ObjectType::Blob) => {
                if !should_index_path(ignore_gs, &rel_str) {
                    continue;
                }
                let blob = repo.find_blob(entry.id())?;
                let bytes = blob.content().to_vec();
                if is_binary_bytes(&bytes) {
                    continue;
                }
                out.push((rel_str, bytes));
            }
            Some(ObjectType::Tree) => {
                if !should_index_path(ignore_gs, &rel_str) {
                    continue;
                }
                let subtree = repo.find_tree(entry.id())?;
                walk_tree(repo, &subtree, &rel_path, ignore_gs, out)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn collect_head_documents(repo_root: &Path, head_oid: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let ignore_gs =
        walker::build_ignore_globset(&[]).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let repo = Repository::open(repo_root)
        .with_context(|| format!("failed to open git repository at {}", repo_root.display()))?;
    let oid = Oid::from_str(head_oid)
        .with_context(|| format!("invalid HEAD oid for instant-grep index: {head_oid}"))?;
    let commit = repo.find_commit(oid)?;
    let tree = commit.tree()?;
    let mut docs = Vec::new();
    walk_tree(&repo, &tree, Path::new(""), &ignore_gs, &mut docs)?;
    let visible_paths = collect_visible_worktree_paths(repo_root, &ignore_gs);
    docs.retain(|(rel_path, _)| visible_paths.contains(rel_path));
    docs.sort_by(|a, b| a.0.cmp(&b.0));
    docs.dedup_by(|a, b| a.0 == b.0);
    Ok(docs)
}

fn collect_visible_worktree_paths(repo_root: &Path, ignore_gs: &GlobSet) -> BTreeSet<String> {
    let mut builder = WalkBuilder::new(repo_root);
    builder.hidden(true);
    builder.git_ignore(true);
    builder.git_global(true);
    builder.git_exclude(true);
    builder.parents(false);
    builder.follow_links(false);

    let root = repo_root.to_path_buf();
    let gs = ignore_gs.clone();
    builder.filter_entry(move |entry| {
        let rel = entry
            .path()
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if rel.is_empty() {
            return true;
        }
        !gs.is_match(&rel)
    });

    let mut paths = BTreeSet::new();
    for entry in builder.build().flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let rel = path
            .strip_prefix(repo_root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if !rel.is_empty() && !ignore_gs.is_match(&rel) {
            paths.insert(rel);
        }
    }

    paths
}

fn encode_uvarint(mut value: u32, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn encode_doc_ids(doc_ids: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut prev = 0u32;
    for &doc_id in doc_ids {
        encode_uvarint(doc_id - prev, &mut out);
        prev = doc_id;
    }
    out
}

fn write_docs_bin(paths: &GenerationPaths, docs: &[String]) -> Result<()> {
    let mut file = fs::File::create(&paths.docs_bin)?;
    for path in docs {
        let bytes = path.as_bytes();
        file.write_all(&(bytes.len() as u32).to_le_bytes())?;
        file.write_all(bytes)?;
    }
    file.sync_all()?;
    Ok(())
}

fn write_postings_and_lookup(
    paths: &GenerationPaths,
    postings: &BTreeMap<GramKey, Vec<u32>>,
) -> Result<()> {
    let mut postings_file = fs::File::create(&paths.postings_bin)?;
    let mut lookup_file = fs::File::create(&paths.lookup_bin)?;
    let mut offset = 0u64;

    for (&gram, doc_ids) in postings {
        let encoded = encode_doc_ids(doc_ids);
        postings_file.write_all(&encoded)?;

        let entry = LookupEntry {
            gram: gram.0,
            postings_offset_bytes: offset,
            postings_len_bytes: encoded.len() as u32,
            doc_freq: doc_ids.len() as u32,
        };

        lookup_file.write_all(&entry.gram.to_le_bytes())?;
        lookup_file.write_all(&entry.postings_offset_bytes.to_le_bytes())?;
        lookup_file.write_all(&entry.postings_len_bytes.to_le_bytes())?;
        lookup_file.write_all(&entry.doc_freq.to_le_bytes())?;

        offset += encoded.len() as u64;
    }

    postings_file.sync_all()?;
    lookup_file.sync_all()?;
    Ok(())
}

fn write_meta(paths: &GenerationPaths, meta: &IndexMeta) -> Result<()> {
    let content = serde_json::to_vec_pretty(meta)?;
    fs::write(&paths.meta_json, content)?;
    Ok(())
}

fn publish_generation(
    root_dir: &Path,
    generation_name: &str,
    temp_dir: &Path,
) -> Result<GenerationPaths> {
    let final_dir = root_dir.join(generation_name);
    if !final_dir.exists() {
        fs::rename(temp_dir, &final_dir)?;
    } else if temp_dir.exists() {
        fs::remove_dir_all(temp_dir)?;
    }

    let current_tmp = root_dir.join("CURRENT.tmp");
    fs::write(&current_tmp, format!("{generation_name}\n"))?;
    fs::rename(current_tmp, root_dir.join("CURRENT"))?;

    Ok(GenerationPaths::new(final_dir))
}

pub fn build_index_for_head(repo_root: &Path, head_oid: &str) -> Result<GenerationPaths> {
    let paths = resolve_index_paths(repo_root)?;
    fs::create_dir_all(&paths.root_dir)?;

    let _lock = FileLock::lock_exclusive(&paths.lock_file).with_context(|| {
        format!(
            "failed to lock instant-grep index at {}",
            paths.lock_file.display()
        )
    })?;

    let generation_name = format!("gen-{head_oid}");
    let final_generation = paths.generation(&generation_name);
    if final_generation.dir.exists() {
        fs::write(&paths.current_file, format!("{generation_name}\n"))?;
        return Ok(final_generation);
    }

    let docs = collect_head_documents(&paths.repo_root, head_oid)?;
    if docs.len() > u32::MAX as usize {
        bail!("instant-grep document count exceeds u32::MAX");
    }

    let mut postings: BTreeMap<GramKey, BTreeSet<u32>> = BTreeMap::new();
    let mut doc_paths = Vec::with_capacity(docs.len());

    for (doc_id, (rel_path, bytes)) in docs.into_iter().enumerate() {
        let doc_id = doc_id as u32;
        doc_paths.push(rel_path);
        for gram in all_grams(&bytes) {
            postings.entry(gram).or_default().insert(doc_id);
        }
    }

    let postings: BTreeMap<GramKey, Vec<u32>> = postings
        .into_iter()
        .map(|(gram, docs)| (gram, docs.into_iter().collect()))
        .collect();

    let temp_dir = paths
        .root_dir
        .join(format!("tmp-{}-{}", generation_name, std::process::id()));
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;
    let temp_generation = GenerationPaths::new(temp_dir.clone());

    let meta = IndexMeta {
        format_version: crate::instant_grep::INDEX_FORMAT_VERSION,
        generation_version: super::GENERATION_VERSION,
        head_oid: head_oid.to_string(),
        repo_root: paths.repo_root.to_string_lossy().to_string(),
        branch_key: paths.branch_key.clone(),
        doc_count: doc_paths.len() as u32,
    };

    write_meta(&temp_generation, &meta)?;
    write_docs_bin(&temp_generation, &doc_paths)?;
    write_postings_and_lookup(&temp_generation, &postings)?;

    publish_generation(&paths.root_dir, &generation_name, &temp_dir)
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    fn commit_file(
        repo: &Repository,
        root: &Path,
        rel: &str,
        content: &str,
        message: &str,
    ) -> String {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new(rel)).unwrap();
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
        let head = commit_file(&repo, tmp.path(), "src/main.rs", "fn main() {}", "init");

        let generation = build_index_for_head(tmp.path(), &head).unwrap();
        assert!(generation.dir.exists());
        assert!(generation.meta_json.exists());
        assert!(generation.docs_bin.exists());
        assert!(generation.lookup_bin.exists());
        assert!(generation.postings_bin.exists());
    }

    #[test]
    fn build_index_filters_head_docs_against_visible_worktree_files() {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let head = commit_file(&repo, tmp.path(), "tracked.txt", "hello\n", "init");

        std::fs::remove_file(tmp.path().join("tracked.txt")).unwrap();

        let generation = build_index_for_head(tmp.path(), &head).unwrap();
        let meta: IndexMeta =
            serde_json::from_slice(&std::fs::read(generation.meta_json).unwrap()).unwrap();
        assert_eq!(meta.doc_count, 0);
    }
}
