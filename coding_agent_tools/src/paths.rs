//! Path normalization utilities for the ls tool.

use std::path::Path;

/// Convert a path to an absolute string representation.
///
/// - If the path exists, returns the canonicalized (resolved) path
/// - If it doesn't exist but is absolute, returns it as-is
/// - If it's relative, joins it with the current directory
pub fn to_abs_string(p: &str) -> String {
    let path = Path::new(p);

    // Try canonicalize first (resolves symlinks, returns real path)
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical.to_string_lossy().to_string();
    }

    // Fall back for non-existent paths
    if path.is_absolute() {
        path.to_string_lossy().to_string()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_becomes_absolute() {
        let abs = to_abs_string("foo/bar");
        assert!(
            Path::new(&abs).is_absolute(),
            "expected absolute path, got: {}",
            abs
        );
    }

    #[test]
    fn absolute_path_stays_absolute() {
        let abs = to_abs_string("/tmp/nonexistent/path");
        assert!(
            Path::new(&abs).is_absolute(),
            "expected absolute path, got: {}",
            abs
        );
        assert!(abs.contains("nonexistent"));
    }

    #[test]
    fn existing_path_canonicalizes() {
        // Current directory always exists
        let abs = to_abs_string(".");
        assert!(Path::new(&abs).is_absolute());
        // Should be canonicalized (no . in the path)
        assert!(!abs.ends_with("/."));
    }
}
