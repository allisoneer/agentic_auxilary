//! Instant-grep index reader.

use super::builder::build_index_for_head;
use super::format::{IndexMeta, LookupEntry};
use super::storage::{GenerationPaths, resolve_index_paths};
use crate::instant_grep::grams::GramKey;
use anyhow::{Context, Result};
use memmap2::Mmap;
use std::fs::File;

const LOOKUP_ENTRY_WIDTH: usize = 24;

pub struct InstantGrepIndex {
    pub meta: IndexMeta,
    _lookup_file: File,
    _postings_file: File,
    lookup_mmap: Mmap,
    postings_mmap: Mmap,
    docs: Vec<String>,
}

fn decode_uvarint(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let mut shift = 0u32;
    let mut value = 0u32;

    while *cursor < bytes.len() {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }

    None
}

fn read_lookup_entry(bytes: &[u8]) -> LookupEntry {
    let mut gram = [0u8; 8];
    gram.copy_from_slice(&bytes[0..8]);
    let mut postings_offset_bytes = [0u8; 8];
    postings_offset_bytes.copy_from_slice(&bytes[8..16]);
    let mut postings_len_bytes = [0u8; 4];
    postings_len_bytes.copy_from_slice(&bytes[16..20]);
    let mut doc_freq = [0u8; 4];
    doc_freq.copy_from_slice(&bytes[20..24]);
    LookupEntry {
        gram: u64::from_le_bytes(gram),
        postings_offset_bytes: u64::from_le_bytes(postings_offset_bytes),
        postings_len_bytes: u32::from_le_bytes(postings_len_bytes),
        doc_freq: u32::from_le_bytes(doc_freq),
    }
}

fn read_docs(paths: &GenerationPaths) -> Result<Vec<String>> {
    let bytes = std::fs::read(&paths.docs_bin)?;
    let mut cursor = 0usize;
    let mut docs = Vec::new();
    while cursor < bytes.len() {
        if cursor + 4 > bytes.len() {
            anyhow::bail!("malformed docs.bin: truncated length prefix");
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&bytes[cursor..cursor + 4]);
        let len = u32::from_le_bytes(len_bytes) as usize;
        cursor += 4;
        if cursor + len > bytes.len() {
            anyhow::bail!("malformed docs.bin: truncated path bytes");
        }
        let path = String::from_utf8(bytes[cursor..cursor + len].to_vec())?;
        cursor += len;
        docs.push(path);
    }
    Ok(docs)
}

impl InstantGrepIndex {
    pub fn open(paths: &GenerationPaths) -> Result<Self> {
        let meta: IndexMeta = serde_json::from_slice(&std::fs::read(&paths.meta_json)?)
            .with_context(|| format!("failed to parse {}", paths.meta_json.display()))?;

        let lookup_file = File::open(&paths.lookup_bin)?;
        let postings_file = File::open(&paths.postings_bin)?;
        // SAFETY: the files are opened read-only and stored in the struct so the
        // underlying file descriptors outlive the memory maps for the lifetime
        // of the reader.
        let lookup_mmap = unsafe { Mmap::map(&lookup_file)? };
        // SAFETY: the files are opened read-only and stored in the struct so the
        // underlying file descriptors outlive the memory maps for the lifetime
        // of the reader.
        let postings_mmap = unsafe { Mmap::map(&postings_file)? };
        let docs = read_docs(paths)?;

        Ok(Self {
            meta,
            _lookup_file: lookup_file,
            _postings_file: postings_file,
            lookup_mmap,
            postings_mmap,
            docs,
        })
    }

    pub fn postings(&self, gram: GramKey) -> Option<Vec<u32>> {
        let mut low = 0usize;
        let mut high = self.lookup_mmap.len() / LOOKUP_ENTRY_WIDTH;

        while low < high {
            let mid = low.midpoint(high);
            let start = mid * LOOKUP_ENTRY_WIDTH;
            let entry = read_lookup_entry(&self.lookup_mmap[start..start + LOOKUP_ENTRY_WIDTH]);
            if entry.gram < gram.0 {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        if low >= self.lookup_mmap.len() / LOOKUP_ENTRY_WIDTH {
            return None;
        }

        let start = low * LOOKUP_ENTRY_WIDTH;
        let entry = read_lookup_entry(&self.lookup_mmap[start..start + LOOKUP_ENTRY_WIDTH]);
        if entry.gram != gram.0 {
            return None;
        }

        let start = entry.postings_offset_bytes as usize;
        let end = start + entry.postings_len_bytes as usize;
        let bytes = &self.postings_mmap[start..end];
        let mut cursor = 0usize;
        let mut prev = 0u32;
        let mut docs = Vec::with_capacity(entry.doc_freq as usize);
        while cursor < bytes.len() {
            let delta = decode_uvarint(bytes, &mut cursor)?;
            prev += delta;
            docs.push(prev);
        }
        Some(docs)
    }

    pub fn doc_path(&self, doc_id: u32) -> &str {
        &self.docs[doc_id as usize]
    }
}

pub fn open_or_build(repo_root: &std::path::Path, head_oid: &str) -> Result<InstantGrepIndex> {
    let paths = resolve_index_paths(repo_root)?;
    if let Some(current) = paths.current_generation()? {
        let index = InstantGrepIndex::open(&current)?;
        if index.meta.format_version == crate::instant_grep::INDEX_FORMAT_VERSION
            && index.meta.generation_version == super::GENERATION_VERSION
            && index.meta.head_oid == head_oid
        {
            return Ok(index);
        }
    }

    let generation = build_index_for_head(repo_root, head_oid)?;
    InstantGrepIndex::open(&generation)
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::instant_grep::grams::all_grams;
    use git2::{Repository, Signature};
    use std::path::Path;
    use tempfile::TempDir;

    fn commit_file(repo: &Repository, root: &std::path::Path, rel: &str, content: &str) -> String {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(rel)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("Test", "test@example.com").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let oid = if let Some(parent) = parent.as_ref() {
            repo.commit(Some("HEAD"), &sig, &sig, "commit", &tree, &[parent])
                .unwrap()
        } else {
            repo.commit(Some("HEAD"), &sig, &sig, "commit", &tree, &[])
                .unwrap()
        };
        oid.to_string()
    }

    #[test]
    fn open_or_build_reads_back_docs_and_postings() {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let head = commit_file(&repo, tmp.path(), "src/main.rs", "fn main() {}\n");

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
        let head = commit_file(&repo, tmp.path(), "src/lib.rs", "pub fn demo() {}\n");

        let _first = open_or_build(tmp.path(), &head).unwrap();
        let paths = resolve_index_paths(tmp.path()).unwrap();
        let first_generation = paths.current_generation_dir().unwrap().unwrap();

        let _second = open_or_build(tmp.path(), &head).unwrap();
        let second_generation = paths.current_generation_dir().unwrap().unwrap();

        assert_eq!(first_generation, second_generation);
    }
}
