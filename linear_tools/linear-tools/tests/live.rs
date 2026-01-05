//! Live API tests for Linear integration.
//!
//! These tests are ignored by default and require:
//! - LINEAR_LIVE_TESTS=1
//! - LINEAR_API_KEY (valid Linear API key)
//! - LINEAR_TEST_TEAM_ID (UUID of a test team to create issues in)
//!
//! Run with: just test-live

use std::time::Duration;
use tokio::time::sleep;

fn live_env_ready() -> bool {
    std::env::var("LINEAR_LIVE_TESTS").ok().as_deref() == Some("1")
        && std::env::var("LINEAR_API_KEY")
            .ok()
            .filter(|v| !v.is_empty())
            .is_some()
        && std::env::var("LINEAR_TEST_TEAM_ID")
            .ok()
            .filter(|v| !v.is_empty())
            .is_some()
}

#[tokio::test]
#[ignore]
async fn live_create_search_read_comment_archive() {
    if !live_env_ready() {
        eprintln!(
            "Skipping live test; set LINEAR_LIVE_TESTS=1 and provide LINEAR_API_KEY + LINEAR_TEST_TEAM_ID"
        );
        return;
    }

    let team_id = std::env::var("LINEAR_TEST_TEAM_ID").unwrap();
    let tools = linear_tools::LinearTools::new();

    let marker = format!(
        "lt-live-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    // 1) Create issue
    let created = tools
        .create_issue(
            team_id.clone(),
            format!("Live Test {}", marker),
            Some("Created by live test".into()),
            Some(3),
            None,
            None,
            None,
            None,
            vec![],
        )
        .await
        .expect("create_issue should succeed");

    assert!(created.success);
    let issue = created.issue.expect("issue should be present");
    let ident = issue.identifier.clone();
    let issue_id = issue.id.clone();

    // 2) Wait for search indexing
    sleep(Duration::from_millis(2000)).await;

    // 3) Search by term
    let search = tools
        .search_issues(
            Some(marker.clone()),
            Some(true), // include_comments
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(10),
            None,
        )
        .await
        .expect("search_issues should succeed");
    assert!(
        search.issues.iter().any(|i| i.identifier == ident),
        "Created issue should be found in search results"
    );

    // 4) Read by identifier
    let details = tools
        .read_issue(ident.clone())
        .await
        .expect("read_issue should succeed");
    assert_eq!(details.issue.identifier, ident);

    // 5) Add comment
    let comment = tools
        .add_comment(ident.clone(), "Live test comment".into(), None)
        .await
        .expect("add_comment should succeed");
    assert!(comment.success);

    // 6) Archive for cleanup
    use cynic::MutationBuilder;
    use linear_queries::mutations::*;
    use linear_tools::http::LinearClient;

    let client = LinearClient::new(None).expect("LinearClient should initialize");
    let op = IssueArchiveMutation::build(IssueArchiveArguments { id: issue_id });
    let resp = client.run(op).await.expect("issueArchive should execute");
    let data = linear_tools::http::extract_data(resp).expect("extract_data should succeed");
    assert!(data.issue_archive.success, "archive should succeed");
}
