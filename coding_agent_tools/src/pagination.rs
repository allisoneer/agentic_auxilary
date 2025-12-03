//! Implicit pagination state for MCP.
//!
//! Stores the last query and offset so that repeated calls with the same
//! parameters advance through pages. State expires after 5 minutes TTL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::types::{LsEntry, Show};

/// Time-to-live for pagination state
const TTL: Duration = Duration::from_secs(5 * 60);

/// Page sizes based on show mode and depth
pub const PAGE_SIZE_ALL: usize = 100;
pub const PAGE_SIZE_FILTERED: usize = 1000;
pub const MAX_DEEP_ENTRIES: usize = 100;

/// State tracking the last query for implicit pagination.
#[derive(Debug, Clone)]
pub struct LastQuery {
    /// Hash key identifying the query parameters
    pub key: String,
    /// Offset for the next page
    pub next_offset: usize,
    /// Page size for this query
    pub page_size: usize,
    /// When this state was created
    pub created_at: Instant,
}

impl LastQuery {
    /// Create a new LastQuery state.
    pub fn new(key: String, page_size: usize) -> Self {
        Self {
            key,
            next_offset: 0,
            page_size,
            created_at: Instant::now(),
        }
    }

    /// Check if this state is still fresh (within TTL).
    pub fn is_fresh(&self) -> bool {
        self.created_at.elapsed() < TTL
    }

    /// Advance to the next page and return the current offset.
    pub fn advance(&mut self) -> usize {
        let current = self.next_offset;
        self.next_offset += self.page_size;
        current
    }
}

/// Generate a cache key from query parameters.
pub fn make_key(root: &str, depth: u8, show: Show, hidden: bool, ignores: &[String]) -> String {
    let show_str = match show {
        Show::All => "all",
        Show::Files => "files",
        Show::Dirs => "dirs",
    };
    format!(
        "root={}|depth={}|show={}|hidden={}|ign={}",
        root,
        depth,
        show_str,
        hidden,
        ignores.join(",")
    )
}

/// Determine the page size based on show mode and depth.
pub fn page_size_for(show: Show, depth: u8) -> usize {
    // Deep queries (depth >= 2) are capped per page but still paginate across calls
    if depth >= 2 {
        MAX_DEEP_ENTRIES
    } else {
        match show {
            Show::All => PAGE_SIZE_ALL,
            Show::Files | Show::Dirs => PAGE_SIZE_FILTERED,
        }
    }
}

/// Apply pagination to a list of entries.
///
/// Returns (paginated_entries, has_more).
pub fn paginate<T>(entries: Vec<T>, offset: usize, page_size: usize) -> (Vec<T>, bool) {
    if offset >= entries.len() {
        return (vec![], false);
    }

    let end = (offset + page_size).min(entries.len());
    let has_more = end < entries.len();

    let paginated: Vec<T> = entries.into_iter().skip(offset).take(page_size).collect();

    (paginated, has_more)
}

// =============================================================================
// Two-level locking cache for parallel-safe pagination
// =============================================================================

/// Two-level locking pagination cache.
///
/// Level 1: Brief lock to get/insert per-query state (outer HashMap)
/// Level 2: Per-query lock held during work, serializes same-param calls
#[derive(Default)]
pub struct PaginationCache {
    map: Mutex<HashMap<String, Arc<QueryLock>>>,
}

impl PaginationCache {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    /// Get or insert the per-query lock for the given key.
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
        let entries: Vec<(String, Arc<QueryLock>)> = {
            let m = self.map.lock().unwrap();
            m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        for (k, lk) in entries {
            let expired = {
                let st = lk.state.lock().unwrap();
                st.is_expired()
            };
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
pub struct QueryLock {
    pub state: Mutex<QueryState>,
}

impl QueryLock {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueryState::empty()),
        }
    }
}

impl Default for QueryLock {
    fn default() -> Self {
        Self::new()
    }
}

/// State for a cached query including full results and pagination offset.
pub struct QueryState {
    /// Cached full results from walker
    pub results: Vec<LsEntry>,
    /// Cached warnings from walker
    pub warnings: Vec<String>,
    /// Next page start offset
    pub next_offset: usize,
    /// Page size for this query
    pub page_size: usize,
    /// When results were (re)computed
    pub created_at: Instant,
}

impl QueryState {
    fn empty() -> Self {
        Self {
            results: Vec::new(),
            warnings: Vec::new(),
            next_offset: 0,
            page_size: 0,
            created_at: Instant::now(),
        }
    }

    /// Reset state with fresh results from walker.
    pub fn reset(&mut self, entries: Vec<LsEntry>, warnings: Vec<String>, page_size: usize) {
        self.results = entries;
        self.warnings = warnings;
        self.next_offset = 0;
        self.page_size = page_size;
        self.created_at = Instant::now();
    }

    /// Check if this state has expired (beyond TTL).
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= TTL
    }

    /// Check if state is empty (never populated).
    pub fn is_empty(&self) -> bool {
        self.results.is_empty() && self.page_size == 0
    }
}

/// Paginate without consuming the entire vector.
///
/// Returns (page_entries, has_more).
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
    fn make_key_generates_consistent_key() {
        let key1 = make_key("/test", 1, Show::All, false, &[]);
        let key2 = make_key("/test", 1, Show::All, false, &[]);
        assert_eq!(key1, key2);

        let key3 = make_key("/test", 1, Show::Files, false, &[]);
        assert_ne!(key1, key3);
    }

    #[test]
    fn page_size_for_shallow_all() {
        assert_eq!(page_size_for(Show::All, 1), PAGE_SIZE_ALL);
    }

    #[test]
    fn page_size_for_shallow_filtered() {
        assert_eq!(page_size_for(Show::Files, 1), PAGE_SIZE_FILTERED);
        assert_eq!(page_size_for(Show::Dirs, 1), PAGE_SIZE_FILTERED);
    }

    #[test]
    fn page_size_for_deep_capped() {
        assert_eq!(page_size_for(Show::All, 2), MAX_DEEP_ENTRIES);
        assert_eq!(page_size_for(Show::Files, 3), MAX_DEEP_ENTRIES);
    }

    #[test]
    fn paginate_first_page() {
        let items: Vec<i32> = (0..150).collect();
        let (page, has_more) = paginate(items, 0, 100);
        assert_eq!(page.len(), 100);
        assert!(has_more);
        assert_eq!(page[0], 0);
        assert_eq!(page[99], 99);
    }

    #[test]
    fn paginate_second_page() {
        let items: Vec<i32> = (0..150).collect();
        let (page, has_more) = paginate(items, 100, 100);
        assert_eq!(page.len(), 50);
        assert!(!has_more);
        assert_eq!(page[0], 100);
        assert_eq!(page[49], 149);
    }

    #[test]
    fn paginate_empty_at_end() {
        let items: Vec<i32> = (0..100).collect();
        let (page, has_more) = paginate(items, 100, 100);
        assert!(page.is_empty());
        assert!(!has_more);
    }

    #[test]
    fn last_query_fresh_within_ttl() {
        let query = LastQuery::new("test".into(), 100);
        assert!(query.is_fresh());
    }

    #[test]
    fn last_query_advance() {
        let mut query = LastQuery::new("test".into(), 100);
        assert_eq!(query.advance(), 0);
        assert_eq!(query.advance(), 100);
        assert_eq!(query.advance(), 200);
    }

    // -------------------------------------------------------------------------
    // Tests for new two-level locking cache infrastructure
    // -------------------------------------------------------------------------

    #[test]
    fn paginate_slice_first_page() {
        let items: Vec<i32> = (0..150).collect();
        let (page, has_more) = paginate_slice(&items, 0, 100);
        assert_eq!(page.len(), 100);
        assert!(has_more);
        assert_eq!(page[0], 0);
        assert_eq!(page[99], 99);
    }

    #[test]
    fn paginate_slice_second_page() {
        let items: Vec<i32> = (0..150).collect();
        let (page, has_more) = paginate_slice(&items, 100, 100);
        assert_eq!(page.len(), 50);
        assert!(!has_more);
        assert_eq!(page[0], 100);
        assert_eq!(page[49], 149);
    }

    #[test]
    fn paginate_slice_empty_at_end() {
        let items: Vec<i32> = (0..100).collect();
        let (page, has_more) = paginate_slice(&items, 100, 100);
        assert!(page.is_empty());
        assert!(!has_more);
    }

    #[test]
    fn query_state_empty_detection() {
        let state = QueryState::empty();
        assert!(state.is_empty());
        assert!(!state.is_expired()); // Fresh state is not expired
    }

    #[test]
    fn query_state_reset() {
        use crate::types::EntryKind;

        let mut state = QueryState::empty();
        assert!(state.is_empty());

        let entries = vec![LsEntry {
            path: "test.txt".into(),
            kind: EntryKind::File,
        }];
        state.reset(entries, vec!["warning".into()], 100);

        assert!(!state.is_empty());
        assert_eq!(state.results.len(), 1);
        assert_eq!(state.warnings.len(), 1);
        assert_eq!(state.page_size, 100);
        assert_eq!(state.next_offset, 0);
    }

    #[test]
    fn pagination_cache_get_or_create() {
        let cache = PaginationCache::new();

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
        let cache = PaginationCache::new();

        let lock1 = cache.get_or_create("key1");

        // Remove with matching Arc should succeed
        cache.remove_if_same("key1", &lock1);

        // New get_or_create should return different Arc
        let lock2 = cache.get_or_create("key1");
        assert!(!Arc::ptr_eq(&lock1, &lock2));
    }

    #[test]
    fn pagination_cache_remove_if_same_ignores_mismatch() {
        let cache = PaginationCache::new();

        let lock1 = cache.get_or_create("key1");

        // Create a different Arc for the same key type
        let different_lock = Arc::new(QueryLock::new());

        // Remove with non-matching Arc should not remove
        cache.remove_if_same("key1", &different_lock);

        // Original lock should still be there
        let lock2 = cache.get_or_create("key1");
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }

    #[test]
    fn sweep_expired_removes_expired_entries() {
        use std::time::Duration;

        let cache = PaginationCache::new();

        // Create an entry
        let lock = cache.get_or_create("key1");

        // Manually expire it by setting created_at to the past
        {
            let mut st = lock.state.lock().unwrap();
            // Set created_at to 6 minutes ago (TTL is 5 minutes)
            st.created_at = std::time::Instant::now() - Duration::from_secs(6 * 60);
        }

        // Sweep should remove expired entry
        cache.sweep_expired();

        // New get_or_create should return a different Arc (old one was removed)
        let lock2 = cache.get_or_create("key1");
        assert!(!Arc::ptr_eq(&lock, &lock2));
    }

    #[test]
    fn sweep_expired_keeps_fresh_entries() {
        let cache = PaginationCache::new();

        // Create an entry (fresh by default)
        let lock1 = cache.get_or_create("key1");

        // Sweep should not remove fresh entries
        cache.sweep_expired();

        // Same Arc should still be there
        let lock2 = cache.get_or_create("key1");
        assert!(Arc::ptr_eq(&lock1, &lock2));
    }
}
