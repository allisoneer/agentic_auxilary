use pr_comments::github::GitHubClient;
use pr_comments::models::GraphQLResponse;
use pr_comments::models::OpenPrRefData;

#[test]
fn parses_open_pr_ref_graphql_payload() {
    let payload = serde_json::json!({
        "data": {
            "repository": {
                "pullRequests": {
                    "nodes": [
                        {
                            "number": 42,
                            "url": "https://github.com/owner/repo/pull/42",
                            "headRefOid": "0123456789abcdef0123456789abcdef01234567"
                        }
                    ]
                }
            }
        }
    });

    let response: GraphQLResponse<OpenPrRefData> = serde_json::from_value(payload).unwrap();
    let node = &response.data.unwrap().repository.pull_requests.nodes[0];

    assert_eq!(node.number, 42);
    assert_eq!(node.head_ref_oid.len(), 40);
}

#[test]
fn parses_check_suites_payload() {
    let payload = serde_json::json!({
        "check_suites": [
            {
                "id": 1,
                "status": "completed",
                "conclusion": "success",
                "app": { "slug": "coderabbitai" },
                "updated_at": "2026-06-24T18:00:00Z"
            }
        ]
    });

    let suites = GitHubClient::parse_check_suites_fixture(payload).unwrap();
    assert_eq!(suites[0].app_slug.as_deref(), Some("coderabbitai"));
    assert_eq!(suites[0].status, "completed");
}

#[test]
fn parses_reviews_and_issue_comments_payloads() {
    let reviews = GitHubClient::parse_reviews_fixture(serde_json::json!([
        {
            "id": 9,
            "state": "COMMENTED",
            "submitted_at": "2026-06-24T18:05:00Z",
            "user": { "login": "coderabbitai[bot]", "type": "Bot" }
        }
    ]))
    .unwrap();
    assert_eq!(reviews[0].user_type.as_deref(), Some("Bot"));

    let comments = GitHubClient::parse_issue_comments_fixture(serde_json::json!([
        {
            "id": 11,
            "body": "Review skipped",
            "created_at": "2026-06-24T18:06:00Z",
            "user": { "login": "coderabbitai[bot]", "type": "Bot" }
        }
    ]))
    .unwrap();
    assert_eq!(comments[0].body, "Review skipped");
}
