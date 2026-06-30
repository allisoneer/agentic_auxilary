use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;

static PROCESS_STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn process_state_lock() -> &'static Mutex<()> {
    PROCESS_STATE_LOCK.get_or_init(|| Mutex::new(()))
}

pub struct CwdGuard {
    previous: PathBuf,
}

impl CwdGuard {
    pub fn pushd(path: &Path) -> std::io::Result<Self> {
        let previous = std::env::current_dir()?;
        std::env::set_current_dir(path)?;
        Ok(Self { previous })
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
    }
}

#[cfg(test)]
pub struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

#[cfg(test)]
impl EnvVarGuard {
    pub fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: test callers serialize process-wide env mutation with process_state_lock.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }

    pub fn remove(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: test callers serialize process-wide env mutation with process_state_lock.
        unsafe { std::env::remove_var(key) };
        Self { key, previous }
    }
}

#[cfg(test)]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(value) => {
                // SAFETY: test callers serialize process-wide env mutation with process_state_lock.
                unsafe { std::env::set_var(self.key, value) };
            }
            None => {
                // SAFETY: test callers serialize process-wide env mutation with process_state_lock.
                unsafe { std::env::remove_var(self.key) };
            }
        }
    }
}
