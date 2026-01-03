//! Stateful pagination for just search results.
//!
//! Two-level locking cache with 5-minute TTL and 10 items per page.

use super::types::SearchItem;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Time-to-live for pagination state
const TTL: Duration = Duration::from_secs(5 * 60);

/// Items per page for search results
pub const PAGE_SIZE: usize = 10;

/// State for a cached query including full results and pagination offset.
pub struct QueryState {
    /// Cached search results
    pub results: Vec<SearchItem>,
    /// Next page start offset
    pub next_offset: usize,
    /// When results were (re)computed
    pub created_at: Instant,
}

impl QueryState {
    fn new() -> Self {
        Self {
            results: vec![],
            next_offset: 0,
            created_at: Instant::now(),
        }
    }

    /// Reset state with fresh results.
    pub fn reset(&mut self, results: Vec<SearchItem>) {
        self.results = results;
        self.next_offset = 0;
        self.created_at = Instant::now();
    }

    /// Check if this state has expired (beyond TTL).
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= TTL
    }

    /// Check if state is empty (never populated).
    pub fn is_empty(&self) -> bool {
        self.results.is_empty() && self.next_offset == 0
    }
}

/// Per-query lock protecting the query state.
pub struct QueryLock {
    pub state: Mutex<QueryState>,
}

impl QueryLock {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueryState::new()),
        }
    }
}

impl Default for QueryLock {
    fn default() -> Self {
        Self::new()
    }
}

/// Two-level locking pagination cache for search results.
///
/// Level 1: Brief lock to get/insert per-query state (outer HashMap)
/// Level 2: Per-query lock held during work, serializes same-param calls
#[derive(Default)]
pub struct PaginationCache {
    map: Mutex<HashMap<String, Arc<QueryLock>>>,
}

impl PaginationCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create the per-query lock for the given key.
    pub fn get_or_create(&self, key: &str) -> Arc<QueryLock> {
        let mut m = self.map.lock().unwrap();
        m.entry(key.to_string())
            .or_insert_with(|| Arc::new(QueryLock::new()))
            .clone()
    }

    /// Remove entry if it still points to the provided Arc.
    pub fn remove_if_same(&self, key: &str, candidate: &Arc<QueryLock>) {
        let mut m = self.map.lock().unwrap();
        if let Some(existing) = m.get(key)
            && Arc::ptr_eq(existing, candidate)
        {
            m.remove(key);
        }
    }

    /// Opportunistic sweep: remove expired entries.
    pub fn sweep_expired(&self) {
        let snapshot: Vec<_> = {
            let m = self.map.lock().unwrap();
            m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };
        for (k, lk) in snapshot {
            let expired = lk.state.lock().unwrap().is_expired();
            if expired {
                self.remove_if_same(&k, &lk);
            }
        }
    }
}

/// Generate a cache key from query parameters.
pub fn make_key(dir: &str, query: &str) -> String {
    format!("dir={}|q={}", dir, query.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_key_consistent() {
        let k1 = make_key("/repo", "build");
        let k2 = make_key("/repo", "BUILD");
        assert_eq!(k1, k2); // case insensitive query
    }

    #[test]
    fn make_key_different_dirs() {
        let k1 = make_key("/repo1", "build");
        let k2 = make_key("/repo2", "build");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_get_or_create() {
        let cache = PaginationCache::new();

        let lock1 = cache.get_or_create("key1");
        let lock2 = cache.get_or_create("key1");
        assert!(Arc::ptr_eq(&lock1, &lock2));

        let lock3 = cache.get_or_create("key2");
        assert!(!Arc::ptr_eq(&lock1, &lock3));
    }

    #[test]
    fn cache_remove_if_same() {
        let cache = PaginationCache::new();

        let lock1 = cache.get_or_create("key1");
        cache.remove_if_same("key1", &lock1);

        let lock2 = cache.get_or_create("key1");
        assert!(!Arc::ptr_eq(&lock1, &lock2));
    }

    #[test]
    fn query_state_lifecycle() {
        let mut state = QueryState::new();
        assert!(state.is_empty());
        assert!(!state.is_expired());

        state.reset(vec![SearchItem {
            recipe: "test".into(),
            dir: "/repo".into(),
            doc: None,
            params: vec![],
        }]);
        assert!(!state.is_empty());
        assert_eq!(state.next_offset, 0);
    }

    #[test]
    fn sweep_removes_expired() {
        let cache = PaginationCache::new();

        let lock = cache.get_or_create("key1");
        {
            let mut st = lock.state.lock().unwrap();
            // Manually expire
            st.created_at = Instant::now() - Duration::from_secs(6 * 60);
        }

        cache.sweep_expired();

        let lock2 = cache.get_or_create("key1");
        assert!(!Arc::ptr_eq(&lock, &lock2));
    }

    #[test]
    fn sweep_keeps_fresh() {
        let cache = PaginationCache::new();

        let lock1 = cache.get_or_create("key1");
        cache.sweep_expired();

        let lock2 = cache.get_or_create("key1");
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }
}
