pub struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    #[must_use]
    pub fn set(key: &'static str, val: &str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: These guards are only used from #[serial(env)] tests.
        unsafe { std::env::set_var(key, val) };
        Self { key, prev }
    }

    #[must_use]
    pub fn remove(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: These guards are only used from #[serial(env)] tests.
        unsafe { std::env::remove_var(key) };
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(value) => {
                // SAFETY: These guards are only used from #[serial(env)] tests.
                unsafe { std::env::set_var(self.key, value) };
            }
            None => {
                // SAFETY: These guards are only used from #[serial(env)] tests.
                unsafe { std::env::remove_var(self.key) };
            }
        }
    }
}
