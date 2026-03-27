use std::time::Duration;

pub const OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS: &str = "OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS";
const DEFAULT_IDLE_GRACE_MS: u64 = 1000;

pub fn idle_grace() -> Duration {
    match std::env::var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS) {
        Ok(value) if !value.trim().is_empty() => {
            if let Ok(ms) = value.trim().parse::<u64>() {
                Duration::from_millis(ms)
            } else {
                tracing::warn!(
                    value = %value,
                    "invalid OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS; using default"
                );
                Duration::from_millis(DEFAULT_IDLE_GRACE_MS)
            }
        }
        _ => Duration::from_millis(DEFAULT_IDLE_GRACE_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn idle_grace_uses_default_when_env_missing() {
        let _guard = lock_env();
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS) };
        assert_eq!(idle_grace(), Duration::from_millis(DEFAULT_IDLE_GRACE_MS));
    }

    #[test]
    fn idle_grace_uses_env_value_when_valid() {
        let _guard = lock_env();
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "42") };
        assert_eq!(idle_grace(), Duration::from_millis(42));
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS) };
    }

    #[test]
    fn idle_grace_falls_back_when_env_invalid() {
        let _guard = lock_env();
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "abc") };
        assert_eq!(idle_grace(), Duration::from_millis(DEFAULT_IDLE_GRACE_MS));
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS) };
    }

    #[test]
    fn idle_grace_falls_back_when_env_empty() {
        let _guard = lock_env();
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS, "   ") };
        assert_eq!(idle_grace(), Duration::from_millis(DEFAULT_IDLE_GRACE_MS));
        // SAFETY: ENV_LOCK serializes process-global environment access in these tests.
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_IDLE_GRACE_MS) };
    }
}
