//! Pagination state helpers specific to pr_comments.
//!
//! Generic pagination infrastructure is in `agentic_tools_utils::pagination`.
//! This module contains pr_comments-specific helpers.

use crate::models::CommentSourceType;

/// Default page size (threads per page)
pub const DEFAULT_PAGE_SIZE: usize = 10;

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

// Re-export pagination types from utils for convenience
pub use agentic_tools_utils::pagination::{
    DEFAULT_TTL, PaginationCache, QueryLock, QueryState, paginate_slice,
};

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
