//! Pagination helpers specific to coding_agent_tools (ls tool).
//!
//! Generic pagination infrastructure is in `agentic_tools_utils::pagination`.
//! This module contains ls-specific helpers and type aliases.

use crate::types::{LsEntry, Show};

/// Page sizes based on show mode and depth
pub const PAGE_SIZE_ALL: usize = 100;
pub const PAGE_SIZE_FILTERED: usize = 1000;
pub const MAX_DEEP_ENTRIES: usize = 100;

// Type aliases for ls pagination that uses warnings as meta
pub type LsQueryState = agentic_tools_utils::pagination::QueryState<LsEntry, Vec<String>>;
pub type LsQueryLock = agentic_tools_utils::pagination::QueryLock<LsEntry, Vec<String>>;
pub type PaginationCache = agentic_tools_utils::pagination::PaginationCache<LsEntry, Vec<String>>;

// Re-export paginate_slice from utils
pub use agentic_tools_utils::pagination::paginate_slice;

/// Paginate by consuming entries (wrapper for backwards compatibility).
///
/// For new code, prefer `paginate_slice` which takes a reference.
pub fn paginate<T>(entries: Vec<T>, offset: usize, page_size: usize) -> (Vec<T>, bool) {
    if offset >= entries.len() {
        return (vec![], false);
    }
    let end = (offset + page_size).min(entries.len());
    let has_more = end < entries.len();
    let paginated: Vec<T> = entries.into_iter().skip(offset).take(page_size).collect();
    (paginated, has_more)
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
}
