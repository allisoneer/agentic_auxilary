//! Test-only utilities for safely mutating process-global state in tests.
//!
//! # Usage
//!
//! ```rust
//! use crate::test_support::{EnvGuard, DirGuard};
//! use serial_test::serial;
//!
//! #[test]
//! #[serial(env)]
//! fn example() {
//!     let _cwd = DirGuard::set(tempdir.path());
//!     let _env = EnvGuard::set("FOO", "bar");
//!     // ... test body ...
//! }
//! ```
//!
//! # Important
//!
//! - All tests that use these guards MUST use `#[serial(env)]` to prevent concurrent
//!   execution and ensure process-global state mutations don't interfere with each other.
//! - Never stack multiple guards of the same variable; prefer separate test functions.

use std::path::{Path, PathBuf};

/// RAII guard for temporarily setting an environment variable.
///
/// The variable is automatically restored to its previous state (or removed if it
/// was not set) when the guard is dropped.
pub(crate) struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    /// Set an environment variable temporarily.
    ///
    /// The previous value (if any) is captured and will be restored when dropped.
    ///
    /// # Safety
    ///
    /// This function uses `unsafe` because `std::env::set_var` can cause data races
    /// if called concurrently with other environment variable operations. However,
    /// this is safe when used with `#[serial(env)]` which ensures no concurrent execution.
    pub(crate) fn set(key: &'static str, val: &str) -> Self {
        let prev = std::env::var(key).ok();
        unsafe { std::env::set_var(key, val) };
        Self { key, prev }
    }

    /// Remove an environment variable temporarily.
    ///
    /// The previous value (if any) is captured and will be restored when dropped.
    ///
    /// # Safety
    ///
    /// This function uses `unsafe` because `std::env::remove_var` can cause data races
    /// if called concurrently with other environment variable operations. However,
    /// this is safe when used with `#[serial(env)]` which ensures no concurrent execution.
    pub(crate) fn remove(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        unsafe { std::env::remove_var(key) };
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

/// RAII guard for temporarily changing the current working directory.
///
/// The directory is automatically restored to its previous value when the guard is dropped.
pub(crate) struct DirGuard {
    prev: PathBuf,
}

impl DirGuard {
    /// Change the current working directory temporarily.
    ///
    /// The previous working directory is captured and will be restored when dropped.
    /// The input path is canonicalized if possible; otherwise the input path is used as-is.
    pub(crate) fn set(to: &Path) -> Self {
        let prev = std::env::current_dir().expect("cwd");
        // Canonicalize if possible; fall back to input path
        let to_canonical = std::fs::canonicalize(to).unwrap_or_else(|_| to.to_path_buf());
        std::env::set_current_dir(&to_canonical)
            .unwrap_or_else(|e| panic!("set_current_dir({:?}) failed: {e}", to_canonical));
        Self { prev }
    }
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.prev).unwrap_or_else(|e| panic!("restore cwd failed: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    #[serial(env)]
    fn envguard_set_and_restore_when_unset() {
        let key = "TEST_SUPPORT_ENVVAR_A";
        let _r = EnvGuard::remove(key);
        {
            let _g = EnvGuard::set(key, "123");
            assert_eq!(std::env::var(key).unwrap(), "123");
        }
        assert!(std::env::var(key).is_err(), "should restore to unset");
    }

    #[test]
    #[serial(env)]
    fn envguard_restore_previous_value() {
        let key = "TEST_SUPPORT_ENVVAR_B";
        let _orig = EnvGuard::set(key, "orig");
        {
            let _g = EnvGuard::set(key, "shadow");
            assert_eq!(std::env::var(key).unwrap(), "shadow");
        }
        assert_eq!(std::env::var(key).unwrap(), "orig");
    }

    #[test]
    #[serial(env)]
    fn envguard_remove_and_restore() {
        let key = "TEST_SUPPORT_ENVVAR_C";
        let _orig = EnvGuard::set(key, "value");
        {
            let _g = EnvGuard::remove(key);
            assert!(std::env::var(key).is_err());
        }
        assert_eq!(std::env::var(key).unwrap(), "value");
    }

    #[test]
    #[serial(env)]
    fn dirguard_sets_and_restores_cwd() {
        let orig = std::env::current_dir().unwrap();
        let td = TempDir::new().unwrap();
        {
            let _gd = DirGuard::set(td.path());
            let got = std::env::current_dir().unwrap();
            // On macOS, /var is a symlink to /private/var. DirGuard::set canonicalizes
            // the target path before setting cwd, so current_dir() returns the canonical
            // path. Canonicalize the tempdir too to avoid a lexical mismatch.
            let td_canonical = std::fs::canonicalize(td.path()).unwrap();
            assert!(
                got.starts_with(&td_canonical),
                "cwd {} should be within canonicalized tempdir {} (raw tempdir: {})",
                got.display(),
                td_canonical.display(),
                td.path().display()
            );
        }
        assert_eq!(std::env::current_dir().unwrap(), orig);
    }
}
