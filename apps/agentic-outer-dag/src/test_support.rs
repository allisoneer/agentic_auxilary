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
