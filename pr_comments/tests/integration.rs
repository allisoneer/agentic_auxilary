use pr_comments::{git, models::*};

#[test]
fn test_url_parsing() {
    let urls = vec![
        ("https://github.com/rust-lang/rust.git", ("rust-lang", "rust")),
        ("git@github.com:rust-lang/rust.git", ("rust-lang", "rust")),
    ];

    for (url, (expected_owner, expected_repo)) in urls {
        let result = git::parse_github_url(url).unwrap();
        assert_eq!(result, (expected_owner.to_string(), expected_repo.to_string()));
    }
}

#[tokio::test]
async fn test_model_serialization() {
    let comment = ReviewComment {
        id: 123,
        user: "testuser".to_string(),
        body: "Test comment".to_string(),
        path: "src/main.rs".to_string(),
        line: Some(42),
        side: Some("RIGHT".to_string()),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        html_url: "https://github.com/owner/repo/pull/1#discussion_r123".to_string(),
        pull_request_review_id: Some(456),
    };

    let json = serde_json::to_string(&comment).unwrap();
    let parsed: ReviewComment = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, comment.id);
}

// Additional integration tests would require mocking GitHub API