//! Path normalization utilities for the ls tool.

use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

const HOME_ERR: &str = "Could not determine home directory. Ensure the HOME environment variable is set or the system can resolve the user's home directory.";

/// Expand tilde (~) to the user's home directory.
///
/// Supports:
/// - "~"  => <home>
/// - "~/" => <home>/(rest)
///
/// Does NOT support "~username".
fn expand_tilde(p: &str) -> Result<PathBuf, String> {
    if p == "~" {
        let home = resolve_home_dir().ok_or_else(|| HOME_ERR.to_string())?;
        return Ok(home);
    }
    if let Some(stripped) = p.strip_prefix("~/") {
        let home = resolve_home_dir().ok_or_else(|| HOME_ERR.to_string())?;
        return Ok(home.join(stripped));
    }
    Ok(PathBuf::from(p))
}

// Extracted for testability; in prod just delegates to dirs.
// Test hooks are always checked but are harmless in production if unset.
fn resolve_home_dir() -> Option<PathBuf> {
    // Test hooks (inert unless env vars are explicitly set):
    // - __CAT_FORCE_HOME_NONE=1 -> force None
    // - __CAT_HOME_FOR_TESTS=<path> -> override home
    if std::env::var("__CAT_FORCE_HOME_NONE").ok().as_deref() == Some("1") {
        return None;
    }
    if let Ok(override_home) = std::env::var("__CAT_HOME_FOR_TESTS") {
        return Some(PathBuf::from(override_home));
    }
    dirs::home_dir()
}

/// Convert a path to an absolute string representation.
///
/// Steps:
/// 1) Expand tilde (~, ~/) before any filesystem operations
/// 2) If the path exists, return the canonicalized (resolved) path
/// 3) If it doesn't exist but is absolute, return it as-is
/// 4) If it's relative, join it with the current directory
pub fn to_abs_string(p: &str) -> Result<String, String> {
    let expanded = expand_tilde(p)?;
    // Try canonicalize first (resolves symlinks, returns real path)
    if let Ok(canonical) = std::fs::canonicalize(&expanded) {
        return Ok(canonical.to_string_lossy().to_string());
    }

    // Fall back for non-existent paths
    let abs = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&expanded))
            .unwrap_or(expanded)
    };
    Ok(abs.to_string_lossy().to_string())
}

pub fn to_abs_string_in_roots(p: &str, roots: &[PathBuf]) -> Result<String, String> {
    if roots.is_empty() {
        return Err("CLI sandbox requires at least one canonical root.".to_string());
    }
    let raw = Path::new(p);
    if raw
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("Path `{p}` contains forbidden `..` traversal."));
    }
    let expanded = expand_tilde(p)?;
    let candidate = if expanded.is_absolute() {
        expanded
    } else {
        roots[0].join(expanded)
    };
    let canonical = std::fs::canonicalize(&candidate)
        .map_err(|error| format!("Failed to canonicalize {}: {error}", candidate.display()))?;
    if !path_is_allowed(&canonical, roots) {
        return Err(format!(
            "Path `{p}` resolves outside the allowed CLI roots."
        ));
    }
    Ok(canonical.to_string_lossy().to_string())
}

pub fn path_is_allowed(path: &Path, roots: &[PathBuf]) -> bool {
    std::fs::canonicalize(path)
        .is_ok_and(|canonical| roots.iter().any(|root| canonical.starts_with(root)))
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::Path;

    #[test]
    fn relative_path_becomes_absolute() {
        let abs = to_abs_string("foo/bar").unwrap();
        assert!(
            Path::new(&abs).is_absolute(),
            "expected absolute path, got: {abs}"
        );
    }

    #[test]
    fn absolute_path_stays_absolute() {
        let abs = to_abs_string("/tmp/nonexistent/path").unwrap();
        assert!(
            Path::new(&abs).is_absolute(),
            "expected absolute path, got: {abs}"
        );
        assert!(abs.contains("nonexistent"));
    }

    #[test]
    fn existing_path_canonicalizes() {
        // Current directory always exists
        let abs = to_abs_string(".").unwrap();
        assert!(Path::new(&abs).is_absolute());
        // Should be canonicalized (no trailing . component)
        assert!(!Path::new(&abs).ends_with(Path::new(".")));
    }

    #[test]
    #[serial]
    fn tilde_slash_expands() {
        // Use test override for deterministic behavior
        // SAFETY: This test is marked #[serial] which ensures no other #[serial] tests
        // run concurrently within this crate. The variable __CAT_HOME_FOR_TESTS is
        // test-specific and cleaned up at test end.
        unsafe {
            std::env::set_var("__CAT_HOME_FOR_TESTS", "/tmp/test_home");
        }
        let abs = to_abs_string("~/").unwrap();
        assert!(Path::new(&abs).is_absolute());
        // Must not start with '~'
        assert!(!abs.starts_with('~'));
        // Should start with our test home
        assert!(abs.starts_with("/tmp/test_home"));
        // SAFETY: Restores environment to prior state. See safety note on corresponding
        // set_var for serialization guarantees.
        unsafe {
            std::env::remove_var("__CAT_HOME_FOR_TESTS");
        }
    }

    #[test]
    #[serial]
    fn tilde_alone_expands() {
        // Use test override for deterministic behavior
        // SAFETY: This test is marked #[serial] which ensures no other #[serial] tests
        // run concurrently within this crate. The variable __CAT_HOME_FOR_TESTS is
        // test-specific and cleaned up at test end.
        unsafe {
            std::env::set_var("__CAT_HOME_FOR_TESTS", "/tmp/test_home");
        }
        let abs = to_abs_string("~").unwrap();
        assert!(Path::new(&abs).is_absolute());
        assert!(!abs.starts_with('~'));
        // Should be exactly our test home (or with trailing slash removed)
        assert!(abs.starts_with("/tmp/test_home"));
        // SAFETY: Restores environment to prior state. See safety note on corresponding
        // set_var for serialization guarantees.
        unsafe {
            std::env::remove_var("__CAT_HOME_FOR_TESTS");
        }
    }

    #[test]
    fn inner_tilde_is_not_expanded() {
        // "~" not at the start should be unchanged
        let out = to_abs_string("some/~/path").unwrap();
        assert!(out.contains('~'));
    }

    #[test]
    #[serial]
    fn error_when_home_unavailable() {
        // Force home resolution to fail
        // SAFETY: This test is marked #[serial] which ensures no other #[serial] tests
        // run concurrently within this crate. The variable __CAT_FORCE_HOME_NONE is
        // test-specific and cleaned up at test end.
        unsafe {
            std::env::set_var("__CAT_FORCE_HOME_NONE", "1");
        }
        let err = to_abs_string("~").unwrap_err();
        assert!(
            err.contains("Could not determine home directory"),
            "unexpected error: {err}"
        );
        // SAFETY: Restores environment to prior state. See safety note on corresponding
        // set_var for serialization guarantees.
        unsafe {
            std::env::remove_var("__CAT_FORCE_HOME_NONE");
        }
    }

    #[test]
    fn sandbox_accepts_multiple_roots_and_rejects_escape() {
        let first = tempfile::TempDir::new().unwrap();
        let second = tempfile::TempDir::new().unwrap();
        std::fs::write(second.path().join("allowed.txt"), "ok").unwrap();
        let roots = vec![
            std::fs::canonicalize(first.path()).unwrap(),
            std::fs::canonicalize(second.path()).unwrap(),
        ];
        let allowed = to_abs_string_in_roots(
            second.path().join("allowed.txt").to_string_lossy().as_ref(),
            &roots,
        )
        .unwrap();
        assert!(allowed.ends_with("allowed.txt"));
        assert!(to_abs_string_in_roots("../escape", &roots).is_err());
        assert!(to_abs_string_in_roots("/", &roots).is_err());
    }

    #[test]
    #[serial]
    fn sandbox_rejects_tilde_expanding_outside_roots() {
        let root = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        // SAFETY: serial test uses a test-only environment key and restores it below.
        unsafe {
            std::env::set_var("__CAT_HOME_FOR_TESTS", outside.path());
        }
        let roots = vec![std::fs::canonicalize(root.path()).unwrap()];
        let result = to_abs_string_in_roots("~", &roots);
        // SAFETY: restores the test-only environment key before returning.
        unsafe {
            std::env::remove_var("__CAT_HOME_FOR_TESTS");
        }
        assert!(result.is_err());
    }
}
