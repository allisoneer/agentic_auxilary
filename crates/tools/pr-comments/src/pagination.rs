//! Pagination state helpers specific to `pr_comments`.
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
    format!("{owner}|{repo}|{pr}|{src_str}|{include_resolved}|{page_size}")
}

/// Generate a cache key for PR list pagination from query parameters.
pub fn make_pr_list_key(owner: &str, repo: &str, state: &str, page_size: usize) -> String {
    format!("{owner}|{repo}|{state}|{page_size}")
}

// Re-export pagination types from utils for convenience
pub use agentic_tools_utils::pagination::DEFAULT_TTL;
pub use agentic_tools_utils::pagination::PaginationCache;
pub use agentic_tools_utils::pagination::QueryLock;
pub use agentic_tools_utils::pagination::QueryState;
pub use agentic_tools_utils::pagination::paginate_slice;

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
    fn make_pr_list_key_generates_consistent_key() {
        let key1 = make_pr_list_key("owner", "repo", "open", 10);
        let key2 = make_pr_list_key("owner", "repo", "open", 10);
        let key3 = make_pr_list_key("owner", "repo", "closed", 10);

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn comment_source_type_serde_roundtrip() {
        use serde_json;

        let robot = CommentSourceType::Robot;
        let human = CommentSourceType::Human;
        let all = CommentSourceType::All;

        // Serialize
        let robot_json = match serde_json::to_string(&robot) {
            Ok(json) => json,
            Err(err) => panic!("robot should serialize: {err}"),
        };
        let human_json = match serde_json::to_string(&human) {
            Ok(json) => json,
            Err(err) => panic!("human should serialize: {err}"),
        };
        let all_json = match serde_json::to_string(&all) {
            Ok(json) => json,
            Err(err) => panic!("all should serialize: {err}"),
        };
        assert_eq!(robot_json, "\"robot\"");
        assert_eq!(human_json, "\"human\"");
        assert_eq!(all_json, "\"all\"");

        // Deserialize
        let robot_roundtrip = match serde_json::from_str::<CommentSourceType>("\"robot\"") {
            Ok(value) => value,
            Err(err) => panic!("robot should deserialize: {err}"),
        };
        let human_roundtrip = match serde_json::from_str::<CommentSourceType>("\"human\"") {
            Ok(value) => value,
            Err(err) => panic!("human should deserialize: {err}"),
        };
        let all_roundtrip = match serde_json::from_str::<CommentSourceType>("\"all\"") {
            Ok(value) => value,
            Err(err) => panic!("all should deserialize: {err}"),
        };
        assert_eq!(robot_roundtrip, CommentSourceType::Robot,);
        assert_eq!(human_roundtrip, CommentSourceType::Human,);
        assert_eq!(all_roundtrip, CommentSourceType::All);
    }

    #[test]
    fn comment_source_type_default() {
        assert_eq!(CommentSourceType::default(), CommentSourceType::All);
    }
}
