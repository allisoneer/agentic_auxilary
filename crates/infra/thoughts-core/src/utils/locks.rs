//! Advisory file locking utilities using fs4.
//!
//! This module provides a thin RAII wrapper around fs4 for advisory file locks.
//! Used for:
//! - Protecting `repos.json` read-modify-write operations
//! - Per-repo clone locks to prevent concurrent clones into the same target

use anyhow::{Context, Result};
use fs4::fs_std::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// RAII advisory file lock.
///
/// The lock is automatically released when this struct is dropped.
/// Uses advisory locking via fs4, which works across processes on Unix systems.
pub struct FileLock {
    _file: File,
    pub path: PathBuf,
}

impl FileLock {
    /// Acquire an exclusive lock, blocking until available.
    ///
    /// Creates the lock file and parent directories if they don't exist.
    pub fn lock_exclusive(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create lock dir: {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("Failed to open lock file: {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("Failed to acquire exclusive lock: {}", path.display()))?;
        Ok(Self { _file: file, path })
    }

    /// Try to acquire an exclusive lock without blocking.
    ///
    /// Returns `Ok(Some(lock))` if the lock was acquired, `Ok(None)` if the lock
    /// is held by another process, or an error for other failures.
    pub fn try_lock_exclusive(path: impl AsRef<Path>) -> Result<Option<Self>> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock_exclusive() {
            Ok(true) => Ok(Some(Self { _file: file, path })),
            Ok(false) => Ok(None), // Lock not acquired (would block)
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_lock_exclusive_basic() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("test.lock");

        let lock = FileLock::lock_exclusive(&lock_path).unwrap();
        assert!(lock_path.exists());
        drop(lock);
    }

    #[test]
    fn test_lock_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("nested").join("dirs").join("test.lock");

        let lock = FileLock::lock_exclusive(&lock_path).unwrap();
        assert!(lock_path.exists());
        drop(lock);
    }

    #[test]
    fn test_try_lock_exclusive_succeeds_when_available() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("test.lock");

        let lock = FileLock::try_lock_exclusive(&lock_path).unwrap();
        assert!(lock.is_some());
    }

    #[test]
    fn test_try_lock_exclusive_fails_when_held() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join("test.lock");

        let _lock1 = FileLock::lock_exclusive(&lock_path).unwrap();
        let lock2 = FileLock::try_lock_exclusive(&lock_path).unwrap();

        assert!(
            lock2.is_none(),
            "Second lock should fail when first is held"
        );
    }
}
