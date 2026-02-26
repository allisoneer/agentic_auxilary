//! Test-only utilities for safely mutating process-global state in tests.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::test_support::EnvGuard;
//! use serial_test::serial;
//!
//! #[test]
//! #[serial]
//! fn example() {
//!     let _guard = EnvGuard::set("__AGENTIC_CONFIG_DIR_FOR_TESTS", "/tmp/test");
//!     // ... test body ...
//! }
//! ```
//!
//! # Important
//!
//! - All tests that use these guards MUST use `#[serial]` to prevent concurrent
//!   execution and ensure process-global state mutations don't interfere with each other.
//! - Never stack multiple guards of the same variable; prefer separate test functions.
//!
// TODO(2): Consolidate env isolation helpers across workspace. Consider shared
// test-support crate to avoid duplicated EnvGuard patterns in agentic-logging,
// coding-agent-tools, etc.

use std::path::Path;

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
    /// this is safe when used with `#[serial]` which ensures no concurrent execution.
    pub(crate) fn set(key: &'static str, val: impl AsRef<Path>) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: Serialized by #[serial] attribute on tests
        unsafe { std::env::set_var(key, val.as_ref().as_os_str()) };
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
    /// this is safe when used with `#[serial]` which ensures no concurrent execution.
    #[allow(dead_code)]
    pub(crate) fn remove(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: Serialized by #[serial] attribute on tests
        unsafe { std::env::remove_var(key) };
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            // SAFETY: Serialized by #[serial] attribute on tests
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn envguard_set_and_restore_when_unset() {
        let key = "__TEST_AGENTIC_CONFIG_ENVVAR_A";
        let _r = EnvGuard::remove(key);
        {
            let _g = EnvGuard::set(key, "/tmp/test");
            assert_eq!(std::env::var(key).unwrap(), "/tmp/test");
        }
        assert!(std::env::var(key).is_err(), "should restore to unset");
    }

    #[test]
    #[serial]
    fn envguard_restore_previous_value() {
        let key = "__TEST_AGENTIC_CONFIG_ENVVAR_B";
        let _orig = EnvGuard::set(key, "/original");
        {
            let _g = EnvGuard::set(key, "/shadow");
            assert_eq!(std::env::var(key).unwrap(), "/shadow");
        }
        assert_eq!(std::env::var(key).unwrap(), "/original");
    }

    #[test]
    #[serial]
    fn envguard_remove_and_restore() {
        let key = "__TEST_AGENTIC_CONFIG_ENVVAR_C";
        let _orig = EnvGuard::set(key, "/value");
        {
            let _g = EnvGuard::remove(key);
            assert!(std::env::var(key).is_err());
        }
        assert_eq!(std::env::var(key).unwrap(), "/value");
    }
}
