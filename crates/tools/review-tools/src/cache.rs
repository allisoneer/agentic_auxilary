//! Snapshot cache with TTL-based expiration.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use crate::types::ReviewSnapshot;

/// Snapshot TTL: 60 minutes (exceeds 30-min session timeout).
pub const SNAPSHOT_TTL: Duration = Duration::from_secs(60 * 60);

/// Thread-safe cache for review snapshots.
#[derive(Default)]
pub struct SnapshotCache {
    map: Mutex<HashMap<String, (Arc<ReviewSnapshot>, Instant)>>,
}

impl SnapshotCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a snapshot and return an Arc to it.
    ///
    /// Also sweeps expired entries on each insert.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned (another thread panicked while holding the lock).
    #[expect(clippy::expect_used, reason = "Mutex poisoning is unrecoverable")]
    pub fn insert(&self, handle: String, snapshot: ReviewSnapshot) -> Arc<ReviewSnapshot> {
        self.sweep_expired();
        let snap = Arc::new(snapshot);
        let mut m = self.map.lock().expect("cache lock poisoned");
        m.insert(handle, (Arc::clone(&snap), Instant::now()));
        snap
    }

    /// Get a snapshot by handle, if it exists and hasn't expired.
    ///
    /// Also sweeps expired entries on each access.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned (another thread panicked while holding the lock).
    #[expect(clippy::expect_used, reason = "Mutex poisoning is unrecoverable")]
    pub fn get(&self, handle: &str) -> Option<Arc<ReviewSnapshot>> {
        self.sweep_expired();
        let m = self.map.lock().expect("cache lock poisoned");
        m.get(handle).map(|(s, _)| Arc::clone(s))
    }

    /// Remove expired entries from the cache.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned (another thread panicked while holding the lock).
    #[expect(clippy::expect_used, reason = "Mutex poisoning is unrecoverable")]
    pub fn sweep_expired(&self) {
        let mut m = self.map.lock().expect("cache lock poisoned");
        m.retain(|_, (_, inserted_at)| inserted_at.elapsed() < SNAPSHOT_TTL);
    }

    /// Get the current number of cached snapshots (for testing).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        let m = self.map.lock().expect("cache lock poisoned");
        m.len()
    }

    /// Check if cache is empty (for testing).
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiffPage;
    use crate::types::DiffStats;
    use std::path::PathBuf;

    fn make_snapshot() -> ReviewSnapshot {
        ReviewSnapshot {
            repo_root: PathBuf::from("/tmp/test"),
            branch_slug: "test-branch".into(),
            base_ref_name: "origin/main".into(),
            pages: vec![DiffPage {
                page: 1,
                content: "diff content".into(),
                files_in_page: vec!["file.rs".into()],
                oversized_warning: None,
            }],
            stats: DiffStats {
                files_changed: 1,
                insertions: 10,
                deletions: 5,
            },
            total_lines: 15,
            page_size_lines: 800,
            changed_files: vec!["file.rs".into()],
        }
    }

    #[test]
    fn insert_and_get_works() {
        let cache = SnapshotCache::new();
        let handle = "test-handle".to_string();
        let snapshot = make_snapshot();

        cache.insert(handle.clone(), snapshot);

        let retrieved = cache.get(&handle);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().branch_slug, "test-branch");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let cache = SnapshotCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn ttl_is_60_minutes() {
        assert_eq!(SNAPSHOT_TTL, Duration::from_secs(3600));
    }
}
