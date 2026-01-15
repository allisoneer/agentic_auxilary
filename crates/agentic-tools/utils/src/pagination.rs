//! Generic two-level locking TTL-based pagination cache.
//!
//! This module provides a thread-safe pagination cache that can be used by
//! MCP servers to implement implicit pagination - where repeated calls with
//! the same parameters automatically advance through pages.
//!
//! # Architecture
//!
//! Uses two-level locking for thread safety:
//! - Level 1: Brief lock on outer HashMap to get/create per-query state
//! - Level 2: Per-query lock held during work, serializing same-param calls
//!
//! # Example
//!
//! ```
//! use agentic_tools_utils::pagination::{PaginationCache, paginate_slice};
//!
//! // Create a cache for your result type
//! let cache: PaginationCache<i32> = PaginationCache::new();
//!
//! // Get or create a lock for a query
//! let lock = cache.get_or_create("my-query-key");
//!
//! // Work with the query state
//! {
//!     let mut state = lock.state.lock().unwrap();
//!     if state.is_empty() {
//!         // Fetch results and populate state
//!         state.reset(vec![1, 2, 3, 4, 5], (), 2);
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default TTL for pagination state: 5 minutes.
pub const DEFAULT_TTL: Duration = Duration::from_secs(5 * 60);

/// Two-level locking pagination cache generic over result T and optional meta M.
///
/// The meta type M allows storing additional per-query context alongside
/// results, such as warnings or metadata from the original query.
#[derive(Default)]
pub struct PaginationCache<T, M = ()> {
    map: Mutex<HashMap<String, Arc<QueryLock<T, M>>>>,
}

impl<T, M> PaginationCache<T, M> {
    /// Create a new empty pagination cache.
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    /// Remove entry if it still points to the provided Arc.
    ///
    /// This is safe for concurrent access - only removes if the current
    /// entry is the exact same Arc, preventing removal of a replaced entry.
    pub fn remove_if_same(&self, key: &str, candidate: &Arc<QueryLock<T, M>>) {
        let mut m = self.map.lock().unwrap();
        if let Some(existing) = m.get(key)
            && Arc::ptr_eq(existing, candidate)
        {
            m.remove(key);
        }
    }
}

impl<T, M: Default> PaginationCache<T, M> {
    /// Get or create the per-query lock for the given key.
    ///
    /// If a lock already exists for this key, returns a clone of its Arc.
    /// Otherwise creates a new QueryLock and returns it.
    pub fn get_or_create(&self, key: &str) -> Arc<QueryLock<T, M>> {
        let mut m = self.map.lock().unwrap();
        m.entry(key.to_string())
            .or_insert_with(|| Arc::new(QueryLock::new()))
            .clone()
    }

    /// Opportunistic sweep: remove expired entries.
    ///
    /// Call this periodically to clean up stale cache entries.
    /// Each expired entry is only removed if it hasn't been replaced.
    pub fn sweep_expired(&self) {
        let entries: Vec<(String, Arc<QueryLock<T, M>>)> = {
            let m = self.map.lock().unwrap();
            m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        for (k, lk) in entries {
            let expired = { lk.state.lock().unwrap().is_expired() };
            if expired {
                let mut m = self.map.lock().unwrap();
                if let Some(existing) = m.get(&k)
                    && Arc::ptr_eq(existing, &lk)
                {
                    m.remove(&k);
                }
            }
        }
    }
}

/// Per-query lock protecting the query state.
pub struct QueryLock<T, M = ()> {
    pub state: Mutex<QueryState<T, M>>,
}

impl<T, M: Default> QueryLock<T, M> {
    /// Create a new QueryLock with empty state.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueryState::with_ttl(DEFAULT_TTL)),
        }
    }
}

impl<T, M: Default> Default for QueryLock<T, M> {
    fn default() -> Self {
        Self::new()
    }
}

/// State for a cached query including full results and pagination offset.
pub struct QueryState<T, M = ()> {
    /// Cached full results
    pub results: Vec<T>,
    /// Optional metadata (e.g., warnings)
    pub meta: M,
    /// Next page start offset
    pub next_offset: usize,
    /// Page size for this query
    pub page_size: usize,
    /// When results were (re)computed
    pub created_at: Instant,
    /// TTL for this state
    ttl: Duration,
}

impl<T> QueryState<T, ()> {
    /// Create empty state with default TTL and unit meta.
    pub fn empty() -> Self {
        Self {
            results: Vec::new(),
            meta: (),
            next_offset: 0,
            page_size: 0,
            created_at: Instant::now(),
            ttl: DEFAULT_TTL,
        }
    }
}

impl<T, M: Default> QueryState<T, M> {
    /// Create empty state with custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            results: Vec::new(),
            meta: M::default(),
            next_offset: 0,
            page_size: 0,
            created_at: Instant::now(),
            ttl,
        }
    }

    /// Reset state with fresh results.
    pub fn reset(&mut self, entries: Vec<T>, meta: M, page_size: usize) {
        self.results = entries;
        self.meta = meta;
        self.next_offset = 0;
        self.page_size = page_size;
        self.created_at = Instant::now();
    }

    /// Check if this state has expired (beyond TTL).
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.ttl
    }

    /// Check if state is empty (never populated).
    pub fn is_empty(&self) -> bool {
        self.results.is_empty() && self.page_size == 0
    }
}

/// Paginate a slice without consuming it.
///
/// Returns (page_entries, has_more).
///
/// # Arguments
/// * `entries` - The full list of entries to paginate
/// * `offset` - Starting offset (0-based)
/// * `page_size` - Maximum entries to return
///
/// # Returns
/// A tuple of (paginated entries, whether more entries remain)
pub fn paginate_slice<T: Clone>(entries: &[T], offset: usize, page_size: usize) -> (Vec<T>, bool) {
    if offset >= entries.len() {
        return (vec![], false);
    }
    let end = (offset + page_size).min(entries.len());
    let has_more = end < entries.len();
    (entries[offset..end].to_vec(), has_more)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_slice_first_page() {
        let items: Vec<i32> = (0..25).collect();
        let (page, has_more) = paginate_slice(&items, 0, 10);
        assert_eq!(page.len(), 10);
        assert!(has_more);
        assert_eq!(page[0], 0);
        assert_eq!(page[9], 9);
    }

    #[test]
    fn paginate_slice_second_page() {
        let items: Vec<i32> = (0..25).collect();
        let (page, has_more) = paginate_slice(&items, 10, 10);
        assert_eq!(page.len(), 10);
        assert!(has_more);
        assert_eq!(page[0], 10);
        assert_eq!(page[9], 19);
    }

    #[test]
    fn paginate_slice_last_page() {
        let items: Vec<i32> = (0..25).collect();
        let (page, has_more) = paginate_slice(&items, 20, 10);
        assert_eq!(page.len(), 5);
        assert!(!has_more);
        assert_eq!(page[0], 20);
        assert_eq!(page[4], 24);
    }

    #[test]
    fn paginate_slice_empty_at_end() {
        let items: Vec<i32> = (0..10).collect();
        let (page, has_more) = paginate_slice(&items, 10, 10);
        assert!(page.is_empty());
        assert!(!has_more);
    }

    #[test]
    fn paginate_slice_empty_input() {
        let items: Vec<i32> = vec![];
        let (page, has_more) = paginate_slice(&items, 0, 10);
        assert!(page.is_empty());
        assert!(!has_more);
    }

    #[test]
    fn query_state_empty_detection() {
        let state: QueryState<i32> = QueryState::empty();
        assert!(state.is_empty());
        assert!(!state.is_expired());
    }

    #[test]
    fn query_state_reset() {
        let mut state: QueryState<i32> = QueryState::empty();
        assert!(state.is_empty());

        state.reset(vec![1, 2, 3], (), 10);
        assert!(!state.is_empty());
        assert_eq!(state.results.len(), 3);
        assert_eq!(state.page_size, 10);
        assert_eq!(state.next_offset, 0);
    }

    #[test]
    fn query_state_with_meta() {
        let mut state: QueryState<i32, Vec<String>> = QueryState::with_ttl(DEFAULT_TTL);
        state.reset(vec![1, 2], vec!["warning".into()], 10);
        assert_eq!(state.meta.len(), 1);
        assert_eq!(state.meta[0], "warning");
    }

    #[test]
    fn pagination_cache_get_or_create() {
        let cache: PaginationCache<i32> = PaginationCache::new();

        // First access creates new entry
        let lock1 = cache.get_or_create("key1");

        // Second access returns same Arc
        let lock2 = cache.get_or_create("key1");
        assert!(Arc::ptr_eq(&lock1, &lock2));

        // Different key creates different entry
        let lock3 = cache.get_or_create("key2");
        assert!(!Arc::ptr_eq(&lock1, &lock3));
    }

    #[test]
    fn pagination_cache_remove_if_same() {
        let cache: PaginationCache<i32> = PaginationCache::new();

        let lock1 = cache.get_or_create("key1");

        // Remove with matching Arc should succeed
        cache.remove_if_same("key1", &lock1);

        // New get_or_create should return different Arc
        let lock2 = cache.get_or_create("key1");
        assert!(!Arc::ptr_eq(&lock1, &lock2));
    }

    #[test]
    fn pagination_cache_remove_if_same_ignores_mismatch() {
        let cache: PaginationCache<i32> = PaginationCache::new();

        let lock1 = cache.get_or_create("key1");

        // Create a different Arc
        let different_lock = Arc::new(QueryLock::<i32>::new());

        // Remove with non-matching Arc should not remove
        cache.remove_if_same("key1", &different_lock);

        // Original lock should still be there
        let lock2 = cache.get_or_create("key1");
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }

    #[test]
    fn sweep_expired_removes_expired_entries() {
        let cache: PaginationCache<i32> = PaginationCache::new();

        // Create an entry
        let lock = cache.get_or_create("key1");

        // Manually expire it by setting created_at to the past
        {
            let mut st = lock.state.lock().unwrap();
            st.created_at = Instant::now() - Duration::from_secs(6 * 60);
        }

        // Sweep should remove expired entry
        cache.sweep_expired();

        // New get_or_create should return a different Arc
        let lock2 = cache.get_or_create("key1");
        assert!(!Arc::ptr_eq(&lock, &lock2));
    }

    #[test]
    fn sweep_expired_keeps_fresh_entries() {
        let cache: PaginationCache<i32> = PaginationCache::new();

        // Create an entry (fresh by default)
        let lock1 = cache.get_or_create("key1");

        // Sweep should not remove fresh entries
        cache.sweep_expired();

        // Same Arc should still be there
        let lock2 = cache.get_or_create("key1");
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }
}
