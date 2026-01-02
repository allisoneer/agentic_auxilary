//! Implicit pagination state for MCP.
//!
//! Stores thread-level pagination with TTL expiration so repeated calls with
//! the same parameters advance through pages. State expires after 5 minutes.

use crate::models::CommentSourceType;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Time-to-live for pagination state
const TTL: Duration = Duration::from_secs(5 * 60);

/// Default page size (threads per page)
pub const DEFAULT_PAGE_SIZE: usize = 10;

// =============================================================================
// Two-level locking cache for parallel-safe pagination
// =============================================================================

/// Two-level locking pagination cache, generic over cached item type T.
///
/// Level 1: Brief lock to get/insert per-query state (outer HashMap)
/// Level 2: Per-query lock held during work, serializes same-param calls
#[derive(Default)]
pub struct PaginationCache<T> {
    map: Mutex<HashMap<String, Arc<QueryLock<T>>>>,
}

impl<T> PaginationCache<T> {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
        }
    }

    /// Get or insert the per-query lock for the given key.
    pub fn get_or_create(&self, key: &str) -> Arc<QueryLock<T>> {
        let mut m = self.map.lock().unwrap();
        m.entry(key.to_string())
            .or_insert_with(|| Arc::new(QueryLock::new()))
            .clone()
    }

    /// Remove entry if it still points to the provided Arc.
    pub fn remove_if_same(&self, key: &str, candidate: &Arc<QueryLock<T>>) {
        let mut m = self.map.lock().unwrap();
        if let Some(existing) = m.get(key)
            && Arc::ptr_eq(existing, candidate)
        {
            m.remove(key);
        }
    }

    /// Opportunistic sweep: remove expired entries.
    pub fn sweep_expired(&self) {
        let entries: Vec<(String, Arc<QueryLock<T>>)> = {
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
pub struct QueryLock<T> {
    pub state: Mutex<QueryState<T>>,
}

impl<T> QueryLock<T> {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueryState::empty()),
        }
    }
}

impl<T> Default for QueryLock<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// State for a cached query including full results and pagination offset.
pub struct QueryState<T> {
    /// Cached full results (e.g., threads)
    pub results: Vec<T>,
    /// Next page start offset
    pub next_offset: usize,
    /// Page size for this query
    pub page_size: usize,
    /// When results were (re)computed
    pub created_at: Instant,
}

impl<T> QueryState<T> {
    fn empty() -> Self {
        Self {
            results: Vec::new(),
            next_offset: 0,
            page_size: 0,
            created_at: Instant::now(),
        }
    }

    /// Reset state with fresh results.
    pub fn reset(&mut self, entries: Vec<T>, page_size: usize) {
        self.results = entries;
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

/// Generate a cache key from query parameters.
pub fn make_key(
    owner: &str,
    repo: &str,
    pr: u64,
    src: CommentSourceType,
    include_resolved: bool,
    page_size: usize,
) -> String {
    let src_str = match src {
        CommentSourceType::Robot => "robot",
        CommentSourceType::Human => "human",
        CommentSourceType::All => "all",
    };
    format!(
        "{}|{}|{}|{}|{}|{}",
        owner, repo, pr, src_str, include_resolved, page_size
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_key_generates_consistent_key() {
        let key1 = make_key("owner", "repo", 123, CommentSourceType::All, false, 10);
        let key2 = make_key("owner", "repo", 123, CommentSourceType::All, false, 10);
        assert_eq!(key1, key2);

        let key3 = make_key("owner", "repo", 123, CommentSourceType::Robot, false, 10);
        assert_ne!(key1, key3);
    }

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
    fn query_state_empty_detection() {
        let state: QueryState<i32> = QueryState::empty();
        assert!(state.is_empty());
        assert!(!state.is_expired());
    }

    #[test]
    fn query_state_reset() {
        let mut state: QueryState<i32> = QueryState::empty();
        assert!(state.is_empty());

        state.reset(vec![1, 2, 3], 10);
        assert!(!state.is_empty());
        assert_eq!(state.results.len(), 3);
        assert_eq!(state.page_size, 10);
        assert_eq!(state.next_offset, 0);
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

    #[test]
    fn comment_source_type_serde_roundtrip() {
        use serde_json;

        let robot = CommentSourceType::Robot;
        let human = CommentSourceType::Human;
        let all = CommentSourceType::All;

        // Serialize
        assert_eq!(serde_json::to_string(&robot).unwrap(), "\"robot\"");
        assert_eq!(serde_json::to_string(&human).unwrap(), "\"human\"");
        assert_eq!(serde_json::to_string(&all).unwrap(), "\"all\"");

        // Deserialize
        assert_eq!(
            serde_json::from_str::<CommentSourceType>("\"robot\"").unwrap(),
            CommentSourceType::Robot
        );
        assert_eq!(
            serde_json::from_str::<CommentSourceType>("\"human\"").unwrap(),
            CommentSourceType::Human
        );
        assert_eq!(
            serde_json::from_str::<CommentSourceType>("\"all\"").unwrap(),
            CommentSourceType::All
        );
    }

    #[test]
    fn comment_source_type_default() {
        assert_eq!(CommentSourceType::default(), CommentSourceType::All);
    }
}
