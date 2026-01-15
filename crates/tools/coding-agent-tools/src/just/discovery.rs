//! Justfile discovery via directory walking.

use walkdir::WalkDir;

/// Path information for a discovered justfile.
#[derive(Debug, Clone)]
pub struct JustfilePath {
    /// Absolute directory containing the justfile
    pub dir: String,
    /// Absolute path to the justfile
    pub path: String,
}

/// Find all justfiles in a repository.
///
/// Walks the directory tree starting from `repo_root_abs`, pruning hidden directories,
/// and returns paths to all discovered Justfile/justfile files.
pub fn find_justfiles(repo_root_abs: &str) -> Result<Vec<JustfilePath>, String> {
    let mut out = Vec::new();
    let root = std::path::Path::new(repo_root_abs);
    let walker = WalkDir::new(repo_root_abs)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Always include the root directory itself
            if e.path() == root {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            if e.file_type().is_dir() {
                // Prune hidden directories (but not the root)
                !name.starts_with('.')
            } else {
                true
            }
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name.eq_ignore_ascii_case("justfile") {
            let path_abs = entry
                .path()
                .canonicalize()
                .map_err(|e| format!("Failed to canonicalize {}: {e}", entry.path().display()))?;
            let dir_abs = path_abs
                .parent()
                .ok_or_else(|| format!("No parent directory for {}", path_abs.display()))?
                .to_path_buf();
            out.push(JustfilePath {
                dir: dir_abs.to_string_lossy().to_string(),
                path: path_abs.to_string_lossy().to_string(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn finds_justfile_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create justfile variants
        fs::write(root.join("justfile"), "default:\n    echo hi").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/Justfile"), "build:\n    cargo build").unwrap();

        let results = find_justfiles(root.to_str().unwrap()).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn prunes_hidden_directories() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create justfile in hidden dir (should be ignored)
        fs::create_dir(root.join(".hidden")).unwrap();
        fs::write(root.join(".hidden/justfile"), "secret:\n    echo secret").unwrap();

        // Create justfile in normal dir
        fs::write(root.join("justfile"), "default:\n    echo hi").unwrap();

        let results = find_justfiles(root.to_str().unwrap()).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].path.contains("justfile"));
        assert!(!results[0].path.contains(".hidden"));
    }

    #[test]
    fn returns_absolute_paths() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("justfile"), "default:\n    echo hi").unwrap();

        let results = find_justfiles(root.to_str().unwrap()).unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            std::path::Path::new(&results[0].path).is_absolute(),
            "path should be absolute: {}",
            results[0].path
        );
        assert!(
            std::path::Path::new(&results[0].dir).is_absolute(),
            "dir should be absolute: {}",
            results[0].dir
        );
    }
}
