//! Repo-local storage paths for instant-grep index generations.

use anyhow::Result;
use std::path::{Path, PathBuf};
use thoughts_tool::git::utils::{get_control_repo_root, get_current_branch};
use thoughts_tool::utils::paths::sanitize_dir_name;

#[derive(Debug, Clone)]
pub struct GenerationPaths {
    pub dir: PathBuf,
    pub meta_json: PathBuf,
    pub docs_bin: PathBuf,
    pub lookup_bin: PathBuf,
    pub postings_bin: PathBuf,
}

impl GenerationPaths {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            meta_json: dir.join("meta.json"),
            docs_bin: dir.join("docs.bin"),
            lookup_bin: dir.join("lookup.bin"),
            postings_bin: dir.join("postings.bin"),
            dir,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexPaths {
    pub repo_root: PathBuf,
    pub branch_key: String,
    pub root_dir: PathBuf,
    pub current_file: PathBuf,
    pub lock_file: PathBuf,
}

impl IndexPaths {
    pub fn current_generation_dir(&self) -> Result<Option<PathBuf>> {
        if !self.current_file.exists() {
            return Ok(None);
        }

        let generation = std::fs::read_to_string(&self.current_file)?
            .trim()
            .to_string();
        if generation.is_empty() {
            return Ok(None);
        }

        Ok(Some(self.root_dir.join(generation)))
    }

    pub fn current_generation(&self) -> Result<Option<GenerationPaths>> {
        Ok(self.current_generation_dir()?.map(GenerationPaths::new))
    }

    pub fn generation(&self, name: &str) -> GenerationPaths {
        GenerationPaths::new(self.root_dir.join(name))
    }
}

pub fn resolve_index_paths(start_path: &Path) -> Result<IndexPaths> {
    let repo_root = get_control_repo_root(start_path)?;
    let branch = get_current_branch(&repo_root)?;
    let branch_key = sanitize_dir_name(&branch);
    let root_dir = repo_root
        .join(".thoughts-data")
        .join("cache")
        .join("instant-grep")
        .join("v1")
        .join("branches")
        .join(&branch_key);

    Ok(IndexPaths {
        current_file: root_dir.join("CURRENT"),
        lock_file: root_dir.join("index.lock"),
        repo_root,
        branch_key,
        root_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_paths_use_expected_filenames() {
        let generation = GenerationPaths::new(PathBuf::from("/tmp/gen-1"));
        assert_eq!(generation.meta_json, PathBuf::from("/tmp/gen-1/meta.json"));
        assert_eq!(generation.docs_bin, PathBuf::from("/tmp/gen-1/docs.bin"));
        assert_eq!(
            generation.lookup_bin,
            PathBuf::from("/tmp/gen-1/lookup.bin")
        );
        assert_eq!(
            generation.postings_bin,
            PathBuf::from("/tmp/gen-1/postings.bin")
        );
    }
}
