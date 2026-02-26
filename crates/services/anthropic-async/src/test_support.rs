//! Test-only utilities for safely mutating process-global state in tests.
//!
//! # Usage
//!
//! ```rust
//! use anthropic_async::test_support::EnvGuard;
//! use serial_test::serial;
//!
//! #[test]
//! #[serial(env)]
//! fn example() {
//!     EnvGuard::with_set("FOO", "bar", || {
//!         // test body runs with FOO=bar
//!     });
//! }
//! ```

/// RAII guard for temporarily setting an environment variable.
///
/// The variable is automatically restored to its previous state (or removed if it
/// was not set) when the guard is dropped.
pub struct EnvGuard {
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
    #[must_use]
    pub fn set(key: &'static str, val: &str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: Callers use #[serial(env)] to ensure no concurrent environment access.
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
    #[must_use]
    pub fn remove(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: Callers use #[serial(env)] to ensure no concurrent environment access.
        unsafe { std::env::remove_var(key) };
        Self { key, prev }
    }

    /// Run a closure with an environment variable temporarily set.
    ///
    /// The variable is set before the closure runs and restored after it completes.
    ///
    /// # Safety
    ///
    /// Same as [`Self::set`] - safe when used with `#[serial(env)]`.
    pub fn with_set<F, R>(key: &'static str, val: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let guard = Self::set(key, val);
        let result = f();
        drop(guard);
        result
    }

    /// Run a closure with an environment variable temporarily removed.
    ///
    /// The variable is removed before the closure runs and restored after it completes.
    ///
    /// # Safety
    ///
    /// Same as [`Self::remove`] - safe when used with `#[serial(env)]`.
    pub fn with_removed<F, R>(key: &'static str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let guard = Self::remove(key);
        let result = f();
        drop(guard);
        result
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => {
                // SAFETY: Callers use #[serial(env)] to ensure no concurrent environment access.
                unsafe { std::env::set_var(self.key, v) }
            }
            None => {
                // SAFETY: Callers use #[serial(env)] to ensure no concurrent environment access.
                unsafe { std::env::remove_var(self.key) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial(env)]
    fn envguard_set_and_restore_when_unset() {
        let key = "TEST_SUPPORT_ENVVAR_A";
        EnvGuard::with_removed(key, || {
            EnvGuard::with_set(key, "123", || {
                assert_eq!(std::env::var(key).unwrap(), "123");
            });
            assert!(std::env::var(key).is_err(), "should restore to unset");
        });
    }

    #[test]
    #[serial(env)]
    fn envguard_restore_previous_value() {
        let key = "TEST_SUPPORT_ENVVAR_B";
        EnvGuard::with_set(key, "orig", || {
            EnvGuard::with_set(key, "shadow", || {
                assert_eq!(std::env::var(key).unwrap(), "shadow");
            });
            assert_eq!(std::env::var(key).unwrap(), "orig");
        });
    }

    #[test]
    #[serial(env)]
    fn envguard_remove_and_restore() {
        let key = "TEST_SUPPORT_ENVVAR_C";
        EnvGuard::with_set(key, "value", || {
            EnvGuard::with_removed(key, || {
                assert!(std::env::var(key).is_err());
            });
            assert_eq!(std::env::var(key).unwrap(), "value");
        });
    }
}
