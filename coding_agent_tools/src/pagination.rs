//! Implicit pagination state for MCP.
//!
//! Stores the last query and offset so that repeated calls with the same
//! parameters advance through pages. State expires after 5 minutes TTL.

use std::time::{Duration, Instant};

use crate::types::Show;

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
    // Deep queries (depth >= 2) are capped, no pagination
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
}
