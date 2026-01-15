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
//!     let _env = EnvGuard::set("FOO", "bar");
//!     // ... test body ...
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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
}
