use pr_comments::{git, models::*};

#[test]
fn test_url_parsing() {
    let urls = vec![
        (
            "https://github.com/rust-lang/rust.git",
            ("rust-lang", "rust"),
        ),
        ("git@github.com:rust-lang/rust.git", ("rust-lang", "rust")),
    ];

    for (url, (expected_owner, expected_repo)) in urls {
        let result = git::parse_github_url(url).unwrap();
        assert_eq!(
            result,
            (expected_owner.to_string(), expected_repo.to_string())
        );
    }
}

#[tokio::test]
async fn test_model_serialization() {
    let comment = ReviewComment {
        id: 123,
        user: "testuser".to_string(),
        is_bot: false,
        body: "Test comment".to_string(),
        path: "src/main.rs".to_string(),
        line: Some(42),
        side: Some("RIGHT".to_string()),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        html_url: "https://github.com/owner/repo/pull/1#discussion_r123".to_string(),
        pull_request_review_id: Some(456),
        in_reply_to_id: None,
    };

    let json = serde_json::to_string(&comment).unwrap();
    let parsed: ReviewComment = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, comment.id);
    assert_eq!(parsed.is_bot, comment.is_bot);
}

// Additional integration tests would require mocking GitHub API

#[cfg(test)]
mod resolution_tests {
    use pr_comments::models::{GraphQLResponse, PullRequestData};

    #[test]
    fn test_include_resolved_default() {
        // Test the default behavior concept used in get_review_comments
        fn get_include_resolved(opt: Option<bool>) -> bool {
            opt.unwrap_or(false)
        }

        // Test that None defaults to false
        assert!(!get_include_resolved(None));
        // Test that Some(true) returns true
        assert!(get_include_resolved(Some(true)));
        // Test that Some(false) returns false
        assert!(!get_include_resolved(Some(false)));
    }

    #[test]
    fn test_graphql_models() {
        // Test that GraphQL response models deserialize correctly
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [{
                                "id": "PRRT_123",
                                "isResolved": true,
                                "comments": {
                                    "nodes": [{
                                        "id": "RC_123",
                                        "databaseId": 456
                                    }]
                                }
                            }],
                            "pageInfo": {
                                "hasNextPage": false,
                                "endCursor": null
                            }
                        }
                    }
                }
            }
        }"#;

        let response: GraphQLResponse<PullRequestData> = serde_json::from_str(json).unwrap();
        assert!(response.data.is_some());
        let data = response.data.unwrap();
        assert_eq!(data.repository.pull_request.review_threads.nodes.len(), 1);
        assert!(data.repository.pull_request.review_threads.nodes[0].is_resolved);
    }
}

#[cfg(test)]
mod filter_pipeline_tests {
    use pr_comments::github::test_helpers::{FilterParams, apply_filters};
    use pr_comments::models::ReviewComment;
    use std::collections::HashMap;

    fn rc_top(id: u64, user: &str) -> ReviewComment {
        ReviewComment {
            id,
            user: user.into(),
            is_bot: false,
            body: format!("P{}", id),
            path: "a.rs".into(),
            line: Some(1),
            side: Some("RIGHT".into()),
            created_at: "2025-01-01T00:00:00Z".into(),
            updated_at: "2025-01-01T00:00:00Z".into(),
            html_url: format!("https://x/{id}"),
            pull_request_review_id: None,
            in_reply_to_id: None,
        }
    }

    fn rc_reply(id: u64, user: &str, parent: u64) -> ReviewComment {
        ReviewComment {
            id,
            user: user.into(),
            is_bot: false,
            body: format!("R{}", id),
            path: "a.rs".into(),
            line: Some(1),
            side: Some("RIGHT".into()),
            created_at: "2025-01-01T00:00:00Z".into(),
            updated_at: "2025-01-01T00:00:00Z".into(),
            html_url: format!("https://x/{id}"),
            pull_request_review_id: None,
            in_reply_to_id: Some(parent),
        }
    }

    fn p(
        include_resolved: bool,
        include_replies: bool,
        author: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> FilterParams<'_> {
        FilterParams {
            include_resolved,
            include_replies,
            author,
            offset: Some(offset),
            limit: Some(limit),
            resolved_ids: HashMap::new(),
        }
    }

    fn ids(xs: &[ReviewComment]) -> Vec<u64> {
        xs.iter().map(|c| c.id).collect()
    }

    // T1 - Bug #1 Reproduction (Offset Skips Parent)
    // Data: [P1(alice), R1(bob→P1), P2(alice)]
    // Params: include_replies=true, author="alice", offset=1
    // Expected: [P2] (R1 must NOT appear)
    #[test]
    fn offset_skips_parent_replies_do_not_leak() {
        let data = vec![
            rc_top(1, "alice"),
            rc_reply(11, "bob", 1),
            rc_top(2, "alice"),
        ];
        let out = apply_filters(data, p(false, true, Some("alice"), 1, 100));
        assert_eq!(ids(&out), vec![2], "reply R11 should not appear without P1");
    }

    // T2 - Bug #3 Reproduction (Limit at Parent)
    // Data: [P1, R1, P2, R2]
    // Params: include_replies=true, limit=2
    // Expected: [P1, R1] (R2 must NOT appear, P2 not in results)
    #[test]
    fn limit_boundary_does_not_start_completion_without_parent_in_results() {
        let data = vec![
            rc_top(1, "u"),
            rc_reply(11, "v", 1),
            rc_top(2, "u"),
            rc_reply(22, "w", 2),
        ];
        let out = apply_filters(data, p(true, true, None, 0, 2));
        assert_eq!(ids(&out), vec![1, 11]);
    }

    // T3 - Thread Completion (Limit Mid-Thread)
    // Data: [P1, R1, R2, P2]
    // Params: include_replies=true, limit=1
    // Expected: [P1, R1, R2] (finish replies to P1 on page)
    #[test]
    fn page_local_thread_completion_includes_all_replies_for_included_parent() {
        let data = vec![
            rc_top(1, "a"),
            rc_reply(11, "b", 1),
            rc_reply(12, "c", 1),
            rc_top(2, "d"),
        ];
        let out = apply_filters(data, p(true, true, None, 0, 1));
        assert_eq!(ids(&out), vec![1, 11, 12]);
    }

    // T4 - Multiple Skipped Parents (HashSet Validation)
    // Data: [P1, R1, P2, R2, P3, R3, P4]
    // Params: include_replies=true, offset=3
    // Expected: [P4] (R1, R2, R3 must NOT appear)
    #[test]
    fn multiple_parents_skipped_by_offset_block_all_their_replies() {
        let data = vec![
            rc_top(1, "a"),
            rc_reply(11, "x", 1),
            rc_top(2, "b"),
            rc_reply(22, "y", 2),
            rc_top(3, "c"),
            rc_reply(33, "z", 3),
            rc_top(4, "d"),
        ];
        let out = apply_filters(data, p(true, true, None, 3, 100));
        assert_eq!(ids(&out), vec![4]);
    }

    // T5 - include_replies=false Unchanged
    // Data: [P1, R1, P2]
    // Params: include_replies=false, offset=1
    // Expected: [P2]
    #[test]
    fn include_replies_false_preserved() {
        let data = vec![rc_top(1, "u"), rc_reply(11, "v", 1), rc_top(2, "w")];
        let out = apply_filters(data, p(true, false, None, 1, 100));
        assert_eq!(ids(&out), vec![2]);
    }

    // T6 - Page-Local Boundary (Intentional Behavior)
    // Data: Page1: [P1, R1], Page2: [R2(→P1)]
    // Params: include_replies=true, limit=1
    // Expected: [P1, R1] (R2 from page2 NOT fetched)
    // Simulate by only feeding page1 entries to the pure helper.
    #[test]
    fn page_local_completion_does_not_fetch_next_page() {
        let page1 = vec![rc_top(1, "u"), rc_reply(11, "v", 1)];
        let out = apply_filters(page1, p(true, true, None, 0, 1));
        assert_eq!(ids(&out), vec![1, 11]);
        // R2 on a later page would not be fetched/added.
    }
}

#[cfg(test)]
mod thread_tests {
    use pr_comments::models::{CommentSourceType, ReviewComment, Thread};
    use std::collections::HashMap;

    fn make_comment(id: u64, user: &str, is_bot: bool, in_reply_to: Option<u64>) -> ReviewComment {
        ReviewComment {
            id,
            user: user.into(),
            is_bot,
            body: format!("Comment {}", id),
            path: "src/lib.rs".into(),
            line: Some(10),
            side: Some("RIGHT".into()),
            created_at: "2025-01-01T00:00:00Z".into(),
            updated_at: "2025-01-01T00:00:00Z".into(),
            html_url: format!("https://example.com/{}", id),
            pull_request_review_id: None,
            in_reply_to_id: in_reply_to,
        }
    }

    fn build_threads(
        comments: Vec<ReviewComment>,
        resolution_map: &HashMap<u64, bool>,
    ) -> Vec<Thread> {
        let mut parents: Vec<ReviewComment> = Vec::new();
        let mut replies_by_parent: HashMap<u64, Vec<ReviewComment>> = HashMap::new();

        for c in comments {
            if let Some(parent_id) = c.in_reply_to_id {
                replies_by_parent.entry(parent_id).or_default().push(c);
            } else {
                parents.push(c);
            }
        }

        parents
            .into_iter()
            .map(|parent| {
                let is_resolved = resolution_map.get(&parent.id).copied().unwrap_or(false);
                let replies = replies_by_parent.remove(&parent.id).unwrap_or_default();
                Thread {
                    parent,
                    replies,
                    is_resolved,
                }
            })
            .collect()
    }

    fn filter_threads(
        threads: Vec<Thread>,
        src: CommentSourceType,
        include_resolved: bool,
    ) -> Vec<Thread> {
        threads
            .into_iter()
            .filter(|thread| {
                if !include_resolved && thread.is_resolved {
                    return false;
                }
                match src {
                    CommentSourceType::Robot => thread.parent.is_bot,
                    CommentSourceType::Human => !thread.parent.is_bot,
                    CommentSourceType::All => true,
                }
            })
            .collect()
    }

    #[test]
    fn build_threads_groups_replies_under_parent() {
        let comments = vec![
            make_comment(1, "alice", false, None),      // parent
            make_comment(2, "bob", false, Some(1)),     // reply to 1
            make_comment(3, "charlie", false, Some(1)), // reply to 1
            make_comment(4, "dave", false, None),       // another parent
        ];
        let res_map = HashMap::new();
        let threads = build_threads(comments, &res_map);

        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].parent.id, 1);
        assert_eq!(threads[0].replies.len(), 2);
        assert_eq!(threads[1].parent.id, 4);
        assert_eq!(threads[1].replies.len(), 0);
    }

    #[test]
    fn filter_threads_by_resolution() {
        let comments = vec![
            make_comment(1, "alice", false, None),
            make_comment(2, "bob", false, None),
        ];
        let mut res_map = HashMap::new();
        res_map.insert(1, true); // thread 1 is resolved
        res_map.insert(2, false); // thread 2 is not resolved

        let threads = build_threads(comments, &res_map);
        let filtered = filter_threads(threads, CommentSourceType::All, false);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].parent.id, 2);
    }

    #[test]
    fn filter_threads_include_resolved() {
        let comments = vec![
            make_comment(1, "alice", false, None),
            make_comment(2, "bob", false, None),
        ];
        let mut res_map = HashMap::new();
        res_map.insert(1, true);
        res_map.insert(2, false);

        let threads = build_threads(comments, &res_map);
        let filtered = filter_threads(threads, CommentSourceType::All, true);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filter_threads_by_robot() {
        let comments = vec![
            make_comment(1, "coderabbit[bot]", true, None), // bot
            make_comment(2, "alice", false, None),          // human
        ];
        let res_map = HashMap::new();

        let threads = build_threads(comments, &res_map);
        let filtered = filter_threads(threads, CommentSourceType::Robot, false);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].parent.id, 1);
    }

    #[test]
    fn filter_threads_by_human() {
        let comments = vec![
            make_comment(1, "coderabbit[bot]", true, None), // bot
            make_comment(2, "alice", false, None),          // human
        ];
        let res_map = HashMap::new();

        let threads = build_threads(comments, &res_map);
        let filtered = filter_threads(threads, CommentSourceType::Human, false);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].parent.id, 2);
    }

    #[test]
    fn filter_threads_all_includes_both() {
        let comments = vec![
            make_comment(1, "coderabbit[bot]", true, None), // bot
            make_comment(2, "alice", false, None),          // human
        ];
        let res_map = HashMap::new();

        let threads = build_threads(comments, &res_map);
        let filtered = filter_threads(threads, CommentSourceType::All, false);

        assert_eq!(filtered.len(), 2);
    }
}
