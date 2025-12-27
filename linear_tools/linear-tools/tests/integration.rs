use linear_tools::test_support::EnvGuard;
use mockito::Server;
use serial_test::serial;

#[tokio::test]
#[serial(env)]
async fn search_issues_auth_failure() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(401)
        .with_body(r#"{"errors":[{"message":"Unauthorized"}]}"#)
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "bad-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.search_issues(None, None, None, None).await;
    assert!(res.is_err());
}

#[tokio::test]
#[serial(env)]
async fn read_issue_by_identifier_success() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "data": {
                "issues": {
                    "nodes": [{
                        "id": "uuid-1",
                        "identifier": "ENG-245",
                        "title": "Test Issue",
                        "description": "Description here",
                        "priority": 2.0,
                        "url": "https://linear.app/test/issue/ENG-245",
                        "createdAt": "2025-01-01T00:00:00Z",
                        "updatedAt": "2025-01-02T00:00:00Z",
                        "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
                        "state": {"id": "s1", "name": "In Progress", "type": "started"},
                        "assignee": null,
                        "project": null
                    }],
                    "pageInfo": {"hasNextPage": false, "endCursor": null}
                }
            }
        }"#,
        )
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.read_issue("ENG-245".into()).await.unwrap();
    assert_eq!(res.issue.identifier, "ENG-245");
    assert_eq!(res.issue.title, "Test Issue");
}

#[tokio::test]
#[serial(env)]
async fn search_issues_success() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "data": {
                "issues": {
                    "nodes": [
                        {
                            "id": "uuid-1",
                            "identifier": "ENG-100",
                            "title": "First Issue",
                            "description": null,
                            "priority": 1.0,
                            "url": "https://linear.app/test/issue/ENG-100",
                            "createdAt": "2025-01-01T00:00:00Z",
                            "updatedAt": "2025-01-02T00:00:00Z",
                            "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
                            "state": {"id": "s1", "name": "Todo", "type": "unstarted"},
                            "assignee": {"id": "u1", "name": "Alice", "displayName": "Alice Smith", "email": "alice@example.com"},
                            "project": null
                        },
                        {
                            "id": "uuid-2",
                            "identifier": "ENG-101",
                            "title": "Second Issue",
                            "description": "Some description",
                            "priority": 2.0,
                            "url": "https://linear.app/test/issue/ENG-101",
                            "createdAt": "2025-01-01T00:00:00Z",
                            "updatedAt": "2025-01-03T00:00:00Z",
                            "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
                            "state": {"id": "s2", "name": "In Progress", "type": "started"},
                            "assignee": null,
                            "project": {"id": "p1", "name": "Q1 Goals"}
                        }
                    ],
                    "pageInfo": {"hasNextPage": true, "endCursor": "cursor123"}
                }
            }
        }"#,
        )
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.search_issues(None, None, None, None).await.unwrap();

    assert_eq!(res.issues.len(), 2);
    assert_eq!(res.issues[0].identifier, "ENG-100");
    assert_eq!(res.issues[0].assignee, Some("Alice Smith".to_string()));
    assert_eq!(res.issues[1].identifier, "ENG-101");
    assert!(res.has_next_page);
    assert_eq!(res.end_cursor, Some("cursor123".to_string()));
}

#[tokio::test]
#[serial(env)]
async fn read_issue_not_found() {
    let mut server = Server::new_async().await;
    // Mock that returns empty results for the identifier search
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json; charset=utf-8")
        .with_body(
            r#"{"data":{"issues":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}"#,
        )
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.read_issue("NONEXISTENT-999".into()).await;
    // Should fail - either with NotFound or with a parsing error depending on mock behavior
    assert!(res.is_err(), "Expected error for non-existent issue");
}

#[tokio::test]
#[serial(env)]
async fn create_issue_success() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "data": {
                "issueCreate": {
                    "success": true,
                    "issue": {
                        "id": "new-uuid",
                        "identifier": "ENG-500",
                        "title": "New Issue",
                        "description": "Issue description",
                        "priority": 2.0,
                        "url": "https://linear.app/test/issue/ENG-500",
                        "createdAt": "2025-01-01T00:00:00Z",
                        "updatedAt": "2025-01-01T00:00:00Z",
                        "team": {"id": "t1", "key": "ENG", "name": "Engineering"},
                        "state": {"id": "s1", "name": "Backlog", "type": "backlog"},
                        "assignee": null,
                        "project": null
                    }
                }
            }
        }"#,
        )
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .create_issue(
            "team-uuid".to_string(),
            "New Issue".to_string(),
            Some("Issue description".to_string()),
            Some(2),
            None,
            None,
        )
        .await
        .unwrap();

    assert!(res.success);
    assert!(res.issue.is_some());
    let issue = res.issue.unwrap();
    assert_eq!(issue.identifier, "ENG-500");
    assert_eq!(issue.title, "New Issue");
}

#[tokio::test]
#[serial(env)]
async fn add_comment_success() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "data": {
                "commentCreate": {
                    "success": true,
                    "comment": {
                        "id": "comment-uuid",
                        "body": "This is a test comment",
                        "createdAt": "2025-01-01T12:00:00Z"
                    }
                }
            }
        }"#,
        )
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .add_comment(
            "issue-uuid".to_string(),
            "This is a test comment".to_string(),
            None,
        )
        .await
        .unwrap();

    assert!(res.success);
    assert_eq!(res.comment_id, Some("comment-uuid".to_string()));
    assert_eq!(res.body, Some("This is a test comment".to_string()));
}
