//! Mtime-based cache for parsed justfile recipes.

use super::discovery::{JustfilePath, find_justfiles};
use super::parser::{ParsedRecipe, parse_justfile};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Clone)]
struct CachedJustfile {
    mtime: SystemTime,
    recipes: Vec<ParsedRecipe>,
}

#[derive(Default)]
struct Inner {
    /// Cache of parsed recipes keyed by justfile path
    files: HashMap<String, CachedJustfile>,
    /// Last discovered justfile paths
    last_paths: Vec<JustfilePath>,
}

/// Registry for caching parsed justfile recipes with mtime-based invalidation.
#[derive(Clone, Default)]
pub struct JustRegistry {
    inner: Arc<Mutex<Inner>>,
}

impl JustRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Refresh the list of discovered justfiles.
    pub async fn refresh(&self, repo_root: &str) -> Result<(), String> {
        let paths = find_justfiles(repo_root)?;
        let mut inner = self.inner.lock().unwrap();
        inner.last_paths = paths;
        Ok(())
    }

    /// Get all recipes from all justfiles in the repository.
    ///
    /// Returns tuples of (directory, recipe) for each discovered recipe.
    /// Automatically refreshes discovery if no paths are cached.
    /// Uses mtime-based invalidation to re-parse changed files.
    pub async fn get_all_recipes(
        &self,
        repo_root: &str,
    ) -> Result<Vec<(String, ParsedRecipe)>, String> {
        // Ensure discovery has run - check without holding lock across await
        let needs_refresh = {
            let inner = self.inner.lock().unwrap();
            inner.last_paths.is_empty()
        };
        if needs_refresh {
            self.refresh(repo_root).await?;
        }

        let paths = self.inner.lock().unwrap().last_paths.clone();
        let mut results = Vec::new();

        for jf in paths {
            let mtime = std::fs::metadata(&jf.path)
                .and_then(|m| m.modified())
                .map_err(|e| format!("stat failed for {}: {e}", jf.path))?;

            let need_parse = {
                let inner = self.inner.lock().unwrap();
                inner
                    .files
                    .get(&jf.path)
                    .map(|c| c.mtime < mtime)
                    .unwrap_or(true)
            };

            if need_parse {
                let recipes = parse_justfile(&jf.path).await?;
                let mut inner = self.inner.lock().unwrap();
                inner
                    .files
                    .insert(jf.path.clone(), CachedJustfile { mtime, recipes });
            }

            let inner = self.inner.lock().unwrap();
            if let Some(cached) = inner.files.get(&jf.path) {
                for r in &cached.recipes {
                    results.push((jf.dir.clone(), r.clone()));
                }
            }
        }
        Ok(results)
    }

    /// Force clear all cached data (useful for testing).
    #[cfg(test)]
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.files.clear();
        inner.last_paths.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn caches_across_calls() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("justfile"), "build:\n    echo building").unwrap();

        let registry = JustRegistry::new();

        // First call parses
        let recipes1 = registry
            .get_all_recipes(root.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(recipes1.len(), 1);

        // Second call uses cache (would fail if reparsing corrupted state)
        let recipes2 = registry
            .get_all_recipes(root.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(recipes2.len(), 1);
    }

    #[tokio::test]
    async fn invalidates_on_mtime_change() {
        // Skip if just not installed
        if tokio::process::Command::new("just")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("Skipping test: just not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let jf = root.join("justfile");
        fs::write(&jf, "build:\n    echo building").unwrap();

        let registry = JustRegistry::new();

        // First call
        let recipes1 = registry
            .get_all_recipes(root.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(recipes1.len(), 1);
        assert_eq!(recipes1[0].1.name, "build");

        // Sleep to ensure different mtime
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify file
        fs::write(&jf, "test:\n    echo testing\n\ncheck:\n    echo checking").unwrap();
        // Touch to ensure mtime changes
        filetime::set_file_mtime(
            &jf,
            filetime::FileTime::from_system_time(std::time::SystemTime::now()),
        )
        .unwrap();

        // Force refresh of paths
        registry.refresh(root.to_str().unwrap()).await.unwrap();

        // Should re-parse and see new recipes
        let recipes2 = registry
            .get_all_recipes(root.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(recipes2.len(), 2);
    }
}
